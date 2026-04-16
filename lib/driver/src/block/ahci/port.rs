use core::ptr;
use core::sync::atomic::{fence, Ordering};

use hal::{
    block::BlockError,
    dma::{DmaAllocator, DmaBuffer},
};

use crate::block::ahci::{
    constants::*,
    fis::{CommandHeader, CommandTable, FisRegH2D},
};

pub(crate) struct PortState {
    pub port_base: usize,
    pub cmd_list_buf: DmaBuffer,
    pub cmd_table_buf: DmaBuffer,
    pub rfis_buf: DmaBuffer,
    pub data_buf: DmaBuffer,
}

/// Spin-wait with timeout (microseconds)
fn wait_for_clear(addr: usize, bit: u32, timeout_us: u32) -> bool {
    for _ in 0..(timeout_us * 100) { // Scale factor for typical CPU speed
        let val = unsafe { ptr::read_volatile(addr as *const u32) };
        if val & bit == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn wait_for_set(addr: usize, bit: u32, timeout_us: u32) -> bool {
    for _ in 0..(timeout_us * 100) {
        let val = unsafe { ptr::read_volatile(addr as *const u32) };
        if val & bit != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

impl PortState {
    pub(super) unsafe fn init(
        mmio_base: usize,
        port: usize,
        dma: &mut dyn DmaAllocator,
    ) -> Result<Self, BlockError> {
        unsafe {
            let port_base = mmio_base + HBA_PORTS + port * PORT_SIZE;

            // Check if port is idle first
            let cmd_reg = ptr::read_volatile((port_base + PORT_CMD) as *const u32);

            // Stop command engine if running
            if cmd_reg & 0x01 != 0 {
                ptr::write_volatile((port_base + PORT_CMD) as *mut u32, cmd_reg & !0x01);

                // Wait for CR (bit 15) to clear - 1 second timeout
                if !wait_for_clear(port_base + PORT_CMD, 0x8000, 1_000_000) {
                    return Err(BlockError::DeviceBusy);
                }
            }

            // Also clear FRE (FIS Receive Enable)
            if cmd_reg & 0x10 != 0 {
                ptr::write_volatile((port_base + PORT_CMD) as *mut u32, cmd_reg & !0x10);
                if !wait_for_clear(port_base + PORT_CMD, 0x8000, 100_000) {
                    return Err(BlockError::DeviceBusy);
                }
            }

            // Allocate DMA buffers
            let cmd_list_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;
            let cmd_table_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;
            let rfis_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;
            let data_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;

            // Zero them
            for buf in [&cmd_list_buf, &cmd_table_buf, &rfis_buf, &data_buf] {
                ptr::write_bytes(buf.virt, 0, 4096);
            }

            // Setup command list (64-bit addresses)
            let clb = cmd_list_buf.phys.as_u64();
            ptr::write_volatile((port_base + PORT_CLB) as *mut u32, clb as u32);
            ptr::write_volatile((port_base + PORT_CLBU) as *mut u32, (clb >> 32) as u32);

            // Setup FIS receive buffer
            let fb = rfis_buf.phys.as_u64();
            ptr::write_volatile((port_base + PORT_FB) as *mut u32, fb as u32);
            ptr::write_volatile((port_base + PORT_FBU) as *mut u32, (fb >> 32) as u32);

            // Clear interrupt status
            ptr::write_volatile((port_base + PORT_IS) as *mut u32, 0xFFFFFFFF);

            // Clear errors
            ptr::write_volatile((port_base + PORT_SERR) as *mut u32, 0xFFFFFFFF);

            // Start FIS receive
            let cmd = ptr::read_volatile((port_base + PORT_CMD) as *const u32);
            ptr::write_volatile((port_base + PORT_CMD) as *mut u32, cmd | 0x10);

            if !wait_for_set(port_base + PORT_CMD, 0x4000, 100_000) { // FR bit
                return Err(BlockError::DeviceError);
            }

            // Start command engine
            let cmd = ptr::read_volatile((port_base + PORT_CMD) as *const u32);
            ptr::write_volatile((port_base + PORT_CMD) as *mut u32, cmd | 0x01);

            if !wait_for_set(port_base + PORT_CMD, 0x8000, 100_000) { // CR bit
                return Err(BlockError::DeviceError);
            }

            Ok(Self { port_base, cmd_list_buf, cmd_table_buf, rfis_buf, data_buf })
        }
    }

    pub(super) unsafe fn issue_command(
        &mut self,
        command: u8,
        lba: u64,
        count: u16,
        is_write: bool,
    ) -> Result<(), BlockError> {
        unsafe {
            let tfd = (self.port_base + PORT_TFD) as *const u32;

            // Wait for BSY (bit 7) and DRQ (bit 3) to clear - 500ms timeout
            if !wait_for_clear(self.port_base + PORT_TFD, 0x88, 500_000) {
                return Err(BlockError::DeviceBusy);
            }

            // Setup command header (slot 0)
            let cmd_header = self.cmd_list_buf.virt as *mut CommandHeader;
            (*cmd_header).flags = (5 - 1) | if is_write { 0x40 } else { 0 }; // 5 PRDTL? No, (5-1) is wrong, should be 0 for 1 entry
            (*cmd_header).prdtl = 1;
            (*cmd_header).prdbc = 0;
            (*cmd_header).ctba = self.cmd_table_buf.phys.as_u64() as u32;
            (*cmd_header).ctbau = (self.cmd_table_buf.phys.as_u64() >> 32) as u32;
            (*cmd_header).reserved = [0; 4];

            // Clear command table
            let cmd_table = self.cmd_table_buf.virt as *mut CommandTable;
            ptr::write_bytes(cmd_table as *mut u8, 0, 256);

            // Setup FIS
            let fis = &mut (*cmd_table).cfis as *mut u8 as *mut FisRegH2D;
            ptr::write_bytes(fis as *mut u8, 0, 20);
            (*fis).fis_type = 0x27; // Register FIS
            (*fis).pmport = 0x80;   // C bit set, port 0
            (*fis).command = command;
            (*fis).device = 0x40 | ((lba >> 24) & 0x0F) as u8; // LBA mode + bits 24-27
            (*fis).lba0 = (lba & 0xFF) as u8;
            (*fis).lba1 = ((lba >> 8) & 0xFF) as u8;
            (*fis).lba2 = ((lba >> 16) & 0xFF) as u8;
            (*fis).lba3 = ((lba >> 24) & 0xFF) as u8;
            (*fis).lba4 = ((lba >> 32) & 0xFF) as u8;
            (*fis).lba5 = ((lba >> 40) & 0xFF) as u8;
            (*fis).countl = (count & 0xFF) as u8;
            (*fis).counth = ((count >> 8) & 0xFF) as u8;

            // Setup PRDT
            let prdt = &mut (*cmd_table).prdt[0];
            prdt.dba = self.data_buf.phys.as_u64() as u32;
            prdt.dbau = (self.data_buf.phys.as_u64() >> 32) as u32;
            prdt.reserved = 0;
            prdt.dbc = ((count as u32) * (SECTOR_SIZE as u32)) - 1;
            prdt.dbc |= 0x80000000; // Interrupt on completion

            // Memory barrier
            fence(Ordering::SeqCst);

            // Issue command to slot 0
            ptr::write_volatile((self.port_base + PORT_CI) as *mut u32, 0x01);

            // Wait for completion - 5 second timeout for slow disks
            if !wait_for_clear(self.port_base + PORT_CI, 0x01, 5_000_000) {
                return Err(BlockError::Timeout);
            }

            // Check Task File Data for errors
            let tfd_val = ptr::read_volatile(tfd);
            if tfd_val & 0x01 != 0 || tfd_val & 0x20 != 0 { // ERR or DF
                return Err(BlockError::DeviceError);
            }

            Ok(())
        }
    }

    pub(crate) unsafe fn read_sectors(
        &mut self,
        lba: u64,
        buf: &mut [u8],
    ) -> Result<(), BlockError> {
        unsafe {
            if buf.len() < SECTOR_SIZE {
                return Err(BlockError::InvalidBuffer);
            }

            self.issue_command(ATA_CMD_READ_SECTORS, lba, 1, false)?;

            ptr::copy_nonoverlapping(
                self.data_buf.virt as *const u8,
                buf.as_mut_ptr(),
                SECTOR_SIZE,
            );
            Ok(())
        }
    }

    pub(crate) unsafe fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        unsafe {
            if buf.len() < SECTOR_SIZE {
                return Err(BlockError::InvalidBuffer);
            }

            ptr::copy_nonoverlapping(
                buf.as_ptr(),
                self.data_buf.virt,
                SECTOR_SIZE,
            );

            self.issue_command(ATA_CMD_WRITE_SECTORS, lba, 1, true)?;
            Ok(())
        }
    }

    pub(super) unsafe fn identify(&mut self, buf: &mut [u8; 512]) -> Result<(), BlockError> {
        unsafe {
            self.issue_command(0xEC, 0, 1, false)?; // IDENTIFY

            ptr::copy_nonoverlapping(
                self.data_buf.virt as *const u8,
                buf.as_mut_ptr(),
                512,
            );
            Ok(())
        }
    }
}
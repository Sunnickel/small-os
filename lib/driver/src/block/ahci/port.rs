use core::ptr;

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

impl PortState {
    pub(super) unsafe fn init(
        mmio_base: usize,
        port: usize,
        dma: &mut dyn DmaAllocator,
    ) -> Result<Self, BlockError> {
        unsafe {
            let port_base = mmio_base + HBA_PORTS + port * PORT_SIZE;

            // Stop command engine
            let cmd = (port_base + PORT_CMD) as *mut u32;
            ptr::write_volatile(cmd, ptr::read_volatile(cmd) & !0x01);
            while ptr::read_volatile(cmd) & 0x8000 != 0 {}

            // Allocate DMA structures
            let cmd_list_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;
            let cmd_table_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;
            let rfis_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;
            let data_buf = dma.alloc(4096, 4096).ok_or(BlockError::NoMemory)?;

            for buf in [&cmd_list_buf, &cmd_table_buf, &rfis_buf, &data_buf] {
                ptr::write_bytes(buf.virt, 0, 4096);
            }

            // Setup command list base (u32 low/high)
            ptr::write_volatile(
                (port_base + PORT_CLB) as *mut u32,
                cmd_list_buf.phys.as_u64() as u32,
            );
            ptr::write_volatile(
                (port_base + PORT_CLBU) as *mut u32,
                (cmd_list_buf.phys.as_u64() >> 32) as u32,
            );

            // Setup FIS base (u32 low/high)
            ptr::write_volatile((port_base + PORT_FB) as *mut u32, rfis_buf.phys.as_u64() as u32);
            ptr::write_volatile(
                (port_base + PORT_FBU) as *mut u32,
                (rfis_buf.phys.as_u64() >> 32) as u32,
            );

            // Clear interrupt status + start command engine
            ptr::write_volatile((port_base + PORT_IS) as *mut u32, 0xFFFF_FFFF);
            ptr::write_volatile(cmd, ptr::read_volatile(cmd) | 0x01 | 0x10);

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

            // Wait for port ready
            while ptr::read_volatile(tfd) & 0x88 != 0 {}

            // Setup command header (slot 0)
            let cmd_header = self.cmd_list_buf.virt as *mut CommandHeader;
            (*cmd_header).flags = (5 - 1) | if is_write { 0x40 } else { 0 };
            (*cmd_header).prdtl = 1;
            (*cmd_header).ctba = self.cmd_table_buf.phys.as_u64() as u32;
            (*cmd_header).ctbau = (self.cmd_table_buf.phys.as_u64() >> 32) as u32;

            // Setup command FIS
            let cmd_table = self.cmd_table_buf.virt as *mut CommandTable;
            ptr::write_bytes(cmd_table as *mut u8, 0, 128);

            let fis = &mut (*cmd_table).cfis as *mut u8 as *mut FisRegH2D;
            (*fis).fis_type = 0x27;
            (*fis).pmport = 0x80;
            (*fis).command = command;
            (*fis).device = 0x40 | ((lba >> 24) & 0x0F) as u8;
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
            prdt.dbc = (count as u32 * SECTOR_SIZE as u32) - 1;

            // Issue + poll
            ptr::write_volatile((self.port_base + PORT_CI) as *mut u32, 1);
            while ptr::read_volatile((self.port_base + PORT_CI) as *const u32) & 1 != 0 {}

            if ptr::read_volatile(tfd) & 0x01 != 0 {
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
            self.issue_command(ATA_CMD_READ_SECTORS, lba, 1, false)?;
            ptr::copy_nonoverlapping(
                self.data_buf.virt as *const u8,
                buf.as_mut_ptr(),
                SECTOR_SIZE.min(buf.len()),
            );
            Ok(())
        }
    }

    pub(crate) unsafe fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), self.data_buf.virt, SECTOR_SIZE.min(buf.len()));
            self.issue_command(ATA_CMD_WRITE_SECTORS, lba, 1, true)?;
            Ok(())
        }
    }

    pub(super) unsafe fn identify(&mut self, buf: &mut [u8; 512]) -> Result<(), BlockError> {
        unsafe {
            self.issue_command(0xEC, 0, 1, false)?;
            ptr::copy_nonoverlapping(self.data_buf.virt as *const u8, buf.as_mut_ptr(), 512);
            Ok(())
        }
    }
}

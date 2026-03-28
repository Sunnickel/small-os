#![no_std]

extern crate alloc;
use driver_core::block_device::{BlockDevice, BlockError};
use driver_core::dma_allocator::DmaAllocator;

const SECTOR_SIZE: usize = 512;

// AHCI memory layout
const HBA_GHC: usize = 0x04;
const HBA_PI: usize = 0x0C;
const HBA_PORTS: usize = 0x100;
const PORT_SIZE: usize = 0x80;

// Port registers
const PORT_CLB: usize = 0x00;
const PORT_CLBU: usize = 0x04;
const PORT_FB: usize = 0x08;
const PORT_FBU: usize = 0x0C;
const PORT_IS: usize = 0x10;
const PORT_IE: usize = 0x14;
const PORT_CMD: usize = 0x18;
const PORT_TFD: usize = 0x20;
const PORT_SIG: usize = 0x24;
const PORT_SSTS: usize = 0x28;
const PORT_SCTL: usize = 0x2C;
const PORT_SERR: usize = 0x30;
const PORT_SACT: usize = 0x34;
const PORT_CI: usize = 0x38;
const PORT_SNTF: usize = 0x3C;

// Command header
#[repr(C, align(128))]
struct CommandHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [u32; 4],
}

// Command FIS (Host to Device)
#[repr(C)]
struct FisRegH2D {
    fis_type: u8,
    pmport: u8,
    command: u8,
    featurel: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    featureh: u8,
    countl: u8,
    counth: u8,
    icc: u8,
    control: u8,
    reserved: [u8; 4],
}

// PRDT entry
#[repr(C)]
struct PrdtEntry {
    dba: u32,
    dbau: u32,
    reserved: u32,
    dbc: u32,
}

// Command table
#[repr(C, align(128))]
struct CommandTable {
    cfis: [u8; 64],
    acmd: [u8; 16],
    reserved: [u8; 48],
    prdt: [PrdtEntry; 1],
}

pub struct AhciDriver {
    mmio_base: usize,
    port: usize,
    sector_count: u64,
    cmd_list_phys: u64,
    cmd_list_virt: usize,
    cmd_table_phys: u64,
    cmd_table_virt: usize,
    rfis_phys: u64,
    rfis_virt: usize,
    data_phys: u64,
    data_virt: usize,
}

impl AhciDriver {
    pub unsafe fn init(mmio_base: usize, dma: &mut impl DmaAllocator) -> Result<Self, BlockError> {
        // Enable AHCI mode
        let ghc = (mmio_base + HBA_GHC) as *mut u32;
        core::ptr::write_volatile(ghc, core::ptr::read_volatile(ghc) | (1 << 31));

        // Find first implemented port
        let pi = core::ptr::read_volatile((mmio_base + HBA_PI) as *const u32);
        let port = (0..32)
            .find(|&i| pi & (1 << i) != 0)
            .ok_or(BlockError::DeviceError)?;

        let mut driver = Self {
            mmio_base,
            port,
            sector_count: 0,
            cmd_list_phys: 0,
            cmd_list_virt: 0,
            cmd_table_phys: 0,
            cmd_table_virt: 0,
            rfis_phys: 0,
            rfis_virt: 0,
            data_phys: 0,
            data_virt: 0,
        };

        driver.init_port(dma)?;
        driver.identify_device()?;

        Ok(driver)
    }

    unsafe fn init_port(&mut self, dma: &mut impl DmaAllocator) -> Result<(), BlockError> {
        let port_base = self.port_base();

        // Stop command engine
        let cmd = (port_base + PORT_CMD) as *mut u32;
        core::ptr::write_volatile(cmd, core::ptr::read_volatile(cmd) & !0x01);
        while core::ptr::read_volatile(cmd) & 0x8000 != 0 {}

        // Allocate DMA structures
        let (cl_phys, cl_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
        let (ct_phys, ct_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
        let (rfis_phys, rfis_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
        let (data_phys, data_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;

        core::ptr::write_bytes(cl_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(ct_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(rfis_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(data_virt as *mut u8, 0, 4096);

        self.cmd_list_phys = cl_phys;
        self.cmd_list_virt = cl_virt;
        self.cmd_table_phys = ct_phys;
        self.cmd_table_virt = ct_virt;
        self.rfis_phys = rfis_phys;
        self.rfis_virt = rfis_virt;
        self.data_phys = data_phys;
        self.data_virt = data_virt;

        // Setup command list base
        core::ptr::write_volatile((port_base + PORT_CLB) as *mut u32, cl_phys as u32);
        core::ptr::write_volatile((port_base + PORT_CLBU) as *mut u32, (cl_phys >> 32) as u32);

        // Setup FIS base
        core::ptr::write_volatile((port_base + PORT_FB) as *mut u32, rfis_phys as u32);
        core::ptr::write_volatile((port_base + PORT_FBU) as *mut u32, (rfis_phys >> 32) as u32);

        // Clear interrupt status
        core::ptr::write_volatile((port_base + PORT_IS) as *mut u32, 0xFFFFFFFF);

        // Start command engine
        core::ptr::write_volatile(cmd, core::ptr::read_volatile(cmd) | 0x01 | 0x10);

        Ok(())
    }

    unsafe fn identify_device(&mut self) -> Result<(), BlockError> {
        // For now, use a default size. In production, issue IDENTIFY DEVICE command.
        self.sector_count = 131071; // 64MB disk
        Ok(())
    }

    fn port_base(&self) -> usize {
        self.mmio_base + HBA_PORTS + self.port * PORT_SIZE
    }

    unsafe fn issue_command(
        &mut self,
        command: u8,
        lba: u64,
        count: u16,
        is_write: bool,
    ) -> Result<(), BlockError> {
        let port_base = self.port_base();

        // Wait for port to be ready
        let tfd = (port_base + PORT_TFD) as *const u32;
        while core::ptr::read_volatile(tfd) & 0x88 != 0 {}

        // Setup command header (slot 0)
        let cmd_header = self.cmd_list_virt as *mut CommandHeader;
        (*cmd_header).flags = (5 - 1) | if is_write { 0x40 } else { 0 };
        (*cmd_header).prdtl = 1;
        (*cmd_header).ctba = self.cmd_table_phys as u32;
        (*cmd_header).ctbau = (self.cmd_table_phys >> 32) as u32;

        // Setup command FIS
        let cmd_table = self.cmd_table_virt as *mut CommandTable;
        core::ptr::write_bytes(cmd_table as *mut u8, 0, 128);

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
        prdt.dba = self.data_phys as u32;
        prdt.dbau = (self.data_phys >> 32) as u32;
        prdt.dbc = (count as u32 * SECTOR_SIZE as u32) - 1;

        // Issue command
        core::ptr::write_volatile((port_base + PORT_CI) as *mut u32, 1);

        // Poll for completion
        while core::ptr::read_volatile((port_base + PORT_CI) as *const u32) & 1 != 0 {}

        // Check for errors
        if core::ptr::read_volatile(tfd) & 0x01 != 0 {
            return Err(BlockError::DeviceError);
        }

        Ok(())
    }

    unsafe fn read_sectors(&mut self, lba: u64, buf: &mut [u8], count: u16) -> Result<(), BlockError> {
        if count == 0 || count > 1 {
            // For simplicity, only single sector reads
            // Full implementation would handle multi-sector
        }

        self.issue_command(0x20, lba, 1, false)?; // READ SECTOR

        core::ptr::copy_nonoverlapping(
            self.data_virt as *const u8,
            buf.as_mut_ptr(),
            SECTOR_SIZE.min(buf.len()),
        );

        Ok(())
    }

    unsafe fn write_sectors(&mut self, lba: u64, buf: &[u8], count: u16) -> Result<(), BlockError> {
        core::ptr::copy_nonoverlapping(
            buf.as_ptr(),
            self.data_virt as *mut u8,
            SECTOR_SIZE.min(buf.len()),
        );

        self.issue_command(0x30, lba, 1, true)?; // WRITE SECTOR

        Ok(())
    }
}

impl BlockDevice for AhciDriver {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds);
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_read = (SECTOR_SIZE - sector_off).min(remaining.len());

            let mut sector_buf = [0u8; SECTOR_SIZE];
            unsafe {
                self.read_sectors(lba, &mut sector_buf, 1)?;
            }
            remaining[..can_read].copy_from_slice(&sector_buf[sector_off..sector_off + can_read]);

            remaining = &mut remaining[can_read..];
            current_offset += can_read as u64;
        }

        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), BlockError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds);
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_write = (SECTOR_SIZE - sector_off).min(remaining.len());

            if sector_off == 0 && can_write == SECTOR_SIZE {
                // Aligned write
                unsafe {
                    self.write_sectors(lba, &remaining[..SECTOR_SIZE], 1)?;
                }
                remaining = &remaining[SECTOR_SIZE..];
                current_offset += SECTOR_SIZE as u64;
            } else {
                // Unaligned - read-modify-write
                let mut sector_buf = [0u8; SECTOR_SIZE];
                unsafe {
                    self.read_sectors(lba, &mut sector_buf, 1)?;
                    sector_buf[sector_off..sector_off + can_write].copy_from_slice(&remaining[..can_write]);
                    self.write_sectors(lba, &sector_buf, 1)?;
                }
                remaining = &remaining[can_write..];
                current_offset += can_write as u64;
            }
        }

        Ok(())
    }

    fn size(&self) -> u64 {
        self.sector_count * SECTOR_SIZE as u64
    }

    fn sector_size(&self) -> usize {
        SECTOR_SIZE
    }
}
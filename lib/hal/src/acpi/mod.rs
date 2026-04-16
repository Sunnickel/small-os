pub mod error;

use core::ptr::NonNull;

use acpi::{AcpiTables, Handler, PhysicalMapping, platform::PciConfigRegions};

use crate::acpi::error::AcpiError;
// ← no more `use crate::pci` here

#[derive(Clone)]
pub struct KernelAcpiHandler {
    pub phys_offset: u64,
}

impl Handler for KernelAcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let virt = (physical_address as u64 + self.phys_offset) as *mut T;
        PhysicalMapping {
            physical_start: physical_address,
            virtual_start: NonNull::new(virt).expect("null phys mapping"),
            region_length: size,
            mapped_length: size,
            handler: self.clone(),
        }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}

    // ── MMIO ─────────────────────────────────────────────────────────────────

    fn read_u8(&self, address: usize) -> u8 {
        unsafe { core::ptr::read_volatile((address as u64 + self.phys_offset) as *const u8) }
    }
    fn read_u16(&self, address: usize) -> u16 {
        unsafe { core::ptr::read_volatile((address as u64 + self.phys_offset) as *const u16) }
    }
    fn read_u32(&self, address: usize) -> u32 {
        unsafe { core::ptr::read_volatile((address as u64 + self.phys_offset) as *const u32) }
    }
    fn read_u64(&self, address: usize) -> u64 {
        unsafe { core::ptr::read_volatile((address as u64 + self.phys_offset) as *const u64) }
    }
    fn write_u8(&self, address: usize, value: u8) {
        unsafe { core::ptr::write_volatile((address as u64 + self.phys_offset) as *mut u8, value) }
    }
    fn write_u16(&self, address: usize, value: u16) {
        unsafe { core::ptr::write_volatile((address as u64 + self.phys_offset) as *mut u16, value) }
    }
    fn write_u32(&self, address: usize, value: u32) {
        unsafe { core::ptr::write_volatile((address as u64 + self.phys_offset) as *mut u32, value) }
    }
    fn write_u64(&self, address: usize, value: u64) {
        unsafe { core::ptr::write_volatile((address as u64 + self.phys_offset) as *mut u64, value) }
    }

    // ── Port I/O ──────────────────────────────────────────────────────────────

    fn read_io_u8(&self, port: u16) -> u8 {
        unsafe { x86_64::instructions::port::Port::new(port).read() }
    }
    fn read_io_u16(&self, port: u16) -> u16 {
        unsafe { x86_64::instructions::port::Port::new(port).read() }
    }
    fn read_io_u32(&self, port: u16) -> u32 {
        unsafe { x86_64::instructions::port::Port::new(port).read() }
    }
    fn write_io_u8(&self, port: u16, value: u8) {
        unsafe { x86_64::instructions::port::Port::new(port).write(value) }
    }
    fn write_io_u16(&self, port: u16, value: u16) {
        unsafe { x86_64::instructions::port::Port::new(port).write(value) }
    }
    fn write_io_u32(&self, port: u16, value: u32) {
        unsafe { x86_64::instructions::port::Port::new(port).write(value) }
    }

    // ── PCI config space ──────────────────────────────────────────────────────
    // Called by the AML interpreter. We route through hal::pci's raw config
    // accessors — these exist purely for this callsite, drivers never use them.

    fn read_pci_u8(&self, address: acpi::PciAddress, offset: u16) -> u8 {
        unsafe {
            crate::pci::config_read8(address.bus(), address.device(), address.function(), offset)
        }
    }
    fn read_pci_u16(&self, address: acpi::PciAddress, offset: u16) -> u16 {
        unsafe {
            crate::pci::config_read16(address.bus(), address.device(), address.function(), offset)
        }
    }
    fn read_pci_u32(&self, address: acpi::PciAddress, offset: u16) -> u32 {
        unsafe {
            crate::pci::config_read32(address.bus(), address.device(), address.function(), offset)
        }
    }

    // Writes not needed — AML doesn't write PCI config for table parsing
    fn write_pci_u8(&self, address: acpi::PciAddress, offset: u16, value: u8) {
        let _ = (address, offset, value);
    }
    fn write_pci_u16(&self, address: acpi::PciAddress, offset: u16, value: u16) {
        let _ = (address, offset, value);
    }
    fn write_pci_u32(&self, address: acpi::PciAddress, offset: u16, value: u32) {
        let _ = (address, offset, value);
    }

    // ── Timing ────────────────────────────────────────────────────────────────

    fn nanos_since_boot(&self) -> u64 { 0 }

    fn stall(&self, microseconds: u64) {
        let cycles = microseconds * 3000;
        for _ in 0..cycles {
            core::hint::spin_loop();
        }
    }

    fn sleep(&self, milliseconds: u64) { self.stall(milliseconds * 1000); }

    // ── AML mutex stubs ───────────────────────────────────────────────────────

    fn create_mutex(&self) -> acpi::Handle { acpi::Handle(0) }
    fn acquire(&self, _mutex: acpi::Handle, _timeout: u16) -> Result<(), acpi::aml::AmlError> {
        Ok(())
    }
    fn release(&self, _mutex: acpi::Handle) {}
}

pub struct AcpiInfo {
    pub ecam_base: u64,
    pub phys_offset: u64,
}

pub fn init_from_rsdp(rsdp_phys: usize, phys_offset: u64) -> Result<AcpiInfo, AcpiError> {
    let handler = KernelAcpiHandler { phys_offset };
    let tables =
        unsafe { AcpiTables::from_rsdp(handler, rsdp_phys).map_err(|_| AcpiError::ParseFailed)? };
    let pci_regions = PciConfigRegions::new(&tables).map_err(|_| AcpiError::NoMcfg)?;
    let ecam_base = pci_regions.physical_address(0, 0, 0, 0).ok_or(AcpiError::NoEcamRegion)?;

    Ok(AcpiInfo { ecam_base, phys_offset })
}

use core::ptr::NonNull;

use acpi::{AcpiTables, Handler, PhysicalMapping, platform::PciConfigRegions};

use crate::{pci, util::debug};

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
        // PhysicalMapping has public fields — construct it directly, no ::new()
        PhysicalMapping {
            physical_start: physical_address,
            virtual_start: NonNull::new(virt).expect("null phys mapping"),
            region_length: size,
            mapped_length: size,
            handler: self.clone(),
        }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {
        // identity map — nothing to unmap
    }

    // ── Memory-mapped I/O ────────────────────────────────────────────────────
    // These are called by the AML interpreter. We're not using AML, but the
    // trait is not optional, so we implement them via volatile reads/writes
    // through our phys_offset mapping.

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

    // ── Port I/O (x86) ───────────────────────────────────────────────────────
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

    // ── PCI config space ─────────────────────────────────────────────────────
    // Route through our own ECAM implementation once init_ecam() has been called.
    // These are used by the AML interpreter for PCI config access; our driver
    // layer uses pci_ecam directly, so these just need to not panic.
    fn read_pci_u8(&self, address: acpi::PciAddress, offset: u16) -> u8 {
        unsafe { pci::ecam_read8(address.bus(), address.device(), address.function(), offset) }
    }
    fn read_pci_u16(&self, address: acpi::PciAddress, offset: u16) -> u16 {
        unsafe { pci::ecam_read16(address.bus(), address.device(), address.function(), offset) }
    }
    fn read_pci_u32(&self, address: acpi::PciAddress, offset: u16) -> u32 {
        unsafe { pci::ecam_read32(address.bus(), address.device(), address.function(), offset) }
    }
    fn write_pci_u8(&self, address: acpi::PciAddress, offset: u16, value: u8) {
        // not needed for our use case
        let _ = (address, offset, value);
    }
    fn write_pci_u16(&self, address: acpi::PciAddress, offset: u16, value: u16) {
        let _ = (address, offset, value);
    }
    fn write_pci_u32(&self, address: acpi::PciAddress, offset: u16, value: u32) {
        let _ = (address, offset, value);
    }

    // ── Timing ───────────────────────────────────────────────────────────────
    fn nanos_since_boot(&self) -> u64 {
        // Minimal stub — not used during table parsing
        0
    }
    fn stall(&self, microseconds: u64) {
        // Busy-wait using a rough cycle count. Good enough for ACPI init.
        // Assumes ~3GHz — ACPI stalls during init are tiny (< 100µs per spec).
        let cycles = microseconds * 3000;
        for _ in 0..cycles {
            core::hint::spin_loop();
        }
    }
    fn sleep(&self, milliseconds: u64) { self.stall(milliseconds * 1000); }

    // ── AML ───────────────────────────────────────────────────────────────
    fn create_mutex(&self) -> acpi::Handle { acpi::Handle(0) }
    fn acquire(&self, _mutex: acpi::Handle, _timeout: u16) -> Result<(), acpi::aml::AmlError> {
        Ok(())
    }
    fn release(&self, _mutex: acpi::Handle) {}
}

pub fn init_from_rsdp(rsdp_phys: usize, phys_offset: u64) {
    let handler = KernelAcpiHandler { phys_offset };

    let tables = unsafe {
        AcpiTables::from_rsdp(handler, rsdp_phys).expect("ACPI: failed to parse tables from RSDP")
    };

    let pci_regions =
        PciConfigRegions::new(&tables).expect("ACPI: no MCFG table — is this a q35 machine?");

    let ecam_base =
        pci_regions.physical_address(0, 0, 0, 0).expect("ACPI: no ECAM region for segment 0 bus 0");

    debug(&alloc::format!("ECAM base = {:#x}, phys_offset = {:#x}", ecam_base, phys_offset));
    debug(&alloc::format!("ACPI MCFG: segment 0 ECAM base = {:#x}", ecam_base,));

    pci::init_ecam(ecam_base, phys_offset);
}

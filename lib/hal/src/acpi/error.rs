#[derive(Debug, Clone, Copy)]
pub enum AcpiError {
    ParseFailed,
    NoMcfg,
    NoEcamRegion,
}

impl AcpiError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ParseFailed => "failed to parse ACPI tables from RSDP",
            Self::NoMcfg => "no MCFG table found — not a Q35/PCIe machine?",
            Self::NoEcamRegion => "no ECAM region for segment 0 bus 0 in MCFG",
        }
    }
}

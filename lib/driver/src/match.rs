/// A single match rule. A driver provides a slice of these;
/// if ANY rule matches a device, the driver is a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchRule {
	/// Match a specific PCI vendor + device ID pair
	PciId { vendor: u16, device: u16 },

	/// Match any device with this PCI class + subclass
	PciClass { class: u8, subclass: u8 },

	/// Match by exact device name (platform/static devices)
	Name(&'static str),
}

impl MatchRule {
	/// Check this rule against a PCI bus device.
	pub fn matches_pci(&self, vendor: u16, device: u16, class: u8, subclass: u8) -> bool {
		match self {
			Self::PciId { vendor: v, device: d } => *v == vendor && *d == device,
			Self::PciClass { class: c, subclass: s } => *c == class && *s == subclass,
			Self::Name(_) => false,
		}
	}

	/// Check this rule against a named platform device.
	pub fn matches_name(&self, name: &str) -> bool {
		match self {
			Self::Name(n) => *n == name,
			_ => false,
		}
	}
}
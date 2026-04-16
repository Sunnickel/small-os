use alloc::format;
// src/fs/vfs/path.rs
use alloc::string::String;
use alloc::vec::Vec;
use super::FsError;

#[derive(Debug, Clone)]
pub struct Path {
	components: Vec<String>,
	absolute: bool,
}

impl Path {
	pub fn new(s: &str) -> Result<Self, FsError> {
		if s.is_empty() {
			return Err(FsError::InvalidPath);
		}

		let absolute = s.starts_with('/');
		let components: Vec<_> = s.split('/')
			.filter(|c| !c.is_empty())
			.map(String::from)
			.collect();

		Ok(Self { components, absolute })
	}

	pub fn components(&self) -> &[String] {
		&self.components
	}

	pub fn is_absolute(&self) -> bool {
		self.absolute
	}

	pub fn parent(&self) -> Option<Self> {
		if self.components.is_empty() {
			return None;
		}
		let mut comps = self.components.clone();
		comps.pop();
		Some(Self {
			components: comps,
			absolute: self.absolute,
		})
	}

	pub fn filename(&self) -> Option<&str> {
		self.components.last().map(|s| s.as_str())
	}

	pub fn to_string(&self) -> String {
		if self.absolute {
			format!("/{}", self.components.join("/"))
		} else {
			self.components.join("/")
		}
	}
}
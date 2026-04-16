// src/fs/vfs/mount.rs
use alloc::{collections::BTreeMap, sync::Arc};
use alloc::string::String;
use super::{FileSystem, Path, FsError};

pub struct MountPoint {
	pub path: Path,
	pub fs: Arc<dyn FileSystem>,
}

pub struct MountTable {
	mounts: BTreeMap<String, Arc<dyn FileSystem>>,
}

impl MountTable {
	pub const fn new() -> Self {
		Self {
			mounts: BTreeMap::new(),
		}
	}

	pub fn mount(&mut self, fs: Arc<dyn FileSystem>, path: Path) -> Result<(), FsError> {
		let key = path.to_string();
		if self.mounts.contains_key(&key) {
			return Err(FsError::MountFailed);
		}
		self.mounts.insert(key, fs);
		Ok(())
	}

	pub fn unmount(&mut self, path: &Path) -> Result<(), FsError> {
		self.mounts.remove(&path.to_string())
			.ok_or(FsError::NotFound)?;
		Ok(())
	}

	/// Find the filesystem and relative path for an absolute path
	pub fn resolve(&self, path: &Path) -> Result<(Arc<dyn FileSystem>, Path), FsError> {
		let path_str = path.to_string();

		// Find longest matching mount prefix
		let mut best_match = None;
		let mut best_len = 0;

		for (mount_path, fs) in &self.mounts {
			if path_str.starts_with(mount_path) && mount_path.len() >= best_len {
				best_len = mount_path.len();
				best_match = Some((mount_path.clone(), fs.clone()));
			}
		}

		let (mount_str, fs) = best_match.ok_or(FsError::NotFound)?;

		// Calculate relative path
		let rel_path = if path_str.len() > mount_str.len() {
			Path::new(&path_str[mount_str.len()..])?
		} else {
			Path::new("/")?
		};

		Ok((fs, rel_path))
	}

	/// Find the filesystem for a given path (longest prefix match)
	pub fn find_mount(&self, path: &Path) -> Result<Arc<dyn FileSystem>, FsError> {
		let path_str = path.to_string();

		// Find longest matching mount prefix
		let mut best_match = None;
		let mut best_len = 0;

		for (mount_path, fs) in &self.mounts {
			if path_str.starts_with(mount_path) && mount_path.len() >= best_len {
				best_len = mount_path.len();
				best_match = Some(fs.clone());
			}
		}

		best_match.ok_or(FsError::NotFound)
	}
}
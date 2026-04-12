use std::fs;

use sha2::{Digest, Sha256};

pub fn hash_file(path: &str) -> String {
    let data = fs::read(path).unwrap_or_else(|_| panic!("missing file: {path}"));

    let mut hasher = Sha256::new();
    hasher.update(&data);

    let result = hasher.finalize();

    hex::encode(result)
}

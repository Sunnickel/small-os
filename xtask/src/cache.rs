use std::{collections::HashMap, fs};

use serde::{Deserialize, Serialize};

const CACHE_PATH: &str = ".build/cache.json";

#[derive(Serialize, Deserialize, Default)]
pub struct BuildCache {
    pub files: HashMap<String, String>,
}

impl BuildCache {
    pub fn load() -> Self {
        fs::read_to_string(CACHE_PATH)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        fs::create_dir_all(".build").ok();
        fs::write(CACHE_PATH, serde_json::to_string_pretty(self).unwrap()).unwrap();
    }

    pub fn is_dirty(&self, file: &str, hash: &str) -> bool {
        match self.files.get(file) {
            Some(old) => old != hash,
            None => true,
        }
    }

    pub fn update(&mut self, file: &str, hash: String) {
        self.files.insert(file.to_string(), hash);
    }
}

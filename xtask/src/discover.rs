use std::path::Path;

fn collect_recursive(dir: &Path, ext: &str, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // skip common junk dirs
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == ".build" {
                    continue;
                }
            }

            collect_recursive(&path, ext, out);
        } else if path.extension().and_then(|e| e.to_str()).map(|e| e == ext).unwrap_or(false) {
            out.push(path.to_string_lossy().to_string());
        }
    }
}

/// Find all `.c` files recursively
pub fn find_c_files(dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    collect_recursive(Path::new(dir), "c", &mut files);

    files.sort();
    files
}

/// Find all `.asm` files recursively
pub fn find_asm_files(dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    collect_recursive(Path::new(dir), "asm", &mut files);

    files.sort();
    files
}

/// Find all `.rs` files recursively
pub fn find_rust_files(dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    collect_recursive(Path::new(format!("{}/src", dir).as_str()), "rs", &mut files);

    files.sort();
    files
}

// commands/mkdir.rs
use alloc::string::{String, ToString};
use alloc::vec;
use macros::commands::command;
use crate::task::shell::{current_dir_path, resolve_path};
use driver::fs::{open, CreateOptions};

#[command(
    name = "mkdir",
    short = "Create directory",
    long = "Create a new directory. Usage: mkdir <dirname>"
)]
async fn mkdir(dirname: String) {
    if dirname.is_empty() {
        println!("mkdir: missing directory name");
        return;
    }

    let current_path = current_dir_path();
    let full_path = resolve_path(&dirname);

    // Get parent directory
    let parent_path = if full_path.contains('/') {
        let last_slash = full_path.rfind('/').unwrap();
        if last_slash == 0 {
            "/".to_string()
        } else {
            full_path[..last_slash].to_string()
        }
    } else {
        current_path
    };

    let dir_name = if full_path.contains('/') {
        full_path.rfind('/').map(|i| &full_path[i+1..]).unwrap_or(&full_path)
    } else {
        &full_path
    };

    // Open parent directory
    let parent = match open(&parent_path) {
        Ok(dir) => {
            if !dir.is_directory() {
                println!("mkdir: not a directory: {}", parent_path);
                return;
            }
            dir
        }
        Err(_) => {
            println!("mkdir: cannot access parent directory: {}", parent_path);
            return;
        }
    };

    // Check if already exists
    if let Ok(_) = open(&full_path) {
        println!("mkdir: already exists: {}", full_path);
        return;
    }

    // Create new directory
    let options = CreateOptions {
        is_directory: true,
        data: vec![], // Directories don't have $DATA
    };

    match driver::fs::create_file(&parent, dir_name, options) {
        Ok(_) => {
            println!("Created directory: {}", full_path);
        }
        Err(e) => {
            println!("mkdir: failed to create directory: {:?}", e);
        }
    }
}
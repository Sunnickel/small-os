use crate::task::shell::{current_dir_path, resolve_path};
use alloc::string::{String, ToString};
use alloc::vec;
use driver::fs::{open, CreateOptions};
use macros::commands::command;

#[command(
    name = "touch",
    short = "Create empty file",
    long = "Create a new empty file. Usage: touch <filename>"
)]
async fn touch(filename: String) {
    if filename.is_empty() {
        println!("touch: missing filename");
        return;
    }

    let current_path = current_dir_path();
    let full_path = resolve_path(&filename);

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

    let file_name = if full_path.contains('/') {
        full_path
            .rfind('/')
            .map(|i| &full_path[i + 1..])
            .unwrap_or(&full_path)
    } else {
        &full_path
    };

    // Open parent directory
    let parent = match open(&parent_path) {
        Ok(dir) => {
            if !dir.is_directory() {
                println!("touch: not a directory: {}", parent_path);
                return;
            }
            dir
        }
        Err(_) => {
            println!("touch: cannot access parent directory: {}", parent_path);
            return;
        }
    };

    // Check if file already exists
    if let Ok(_) = open(&full_path) {
        // File exists, update timestamp (not implemented, just succeed)
        return;
    }

    // Create new empty file
    let options = CreateOptions {
        is_directory: false,
        data: vec![], // Empty file
    };

    match driver::fs::create_file(&parent, file_name, options) {
        Ok(_) => {
            println!("Created file: {}", full_path);
        }
        Err(e) => {
            println!("touch: failed to create file: {:?}", e);
        }
    }
}

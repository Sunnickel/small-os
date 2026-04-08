use alloc::{
    string::{String, ToString},
    vec,
};

use driver::fs::{fs_mutex, is_initialized, CreateOptions};
use macros::command;

use crate::task::shell::{current_dir_path, resolve_path};

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

    // Check filesystem is initialized
    if !is_initialized() {
        println!("touch: filesystem not initialized");
        return;
    }

    let current_path = current_dir_path();
    let full_path = resolve_path(&filename);

    // Get parent directory path and filename
    let (parent_path, file_name) = if full_path == "/" {
        println!("touch: cannot create file at root without name");
        return;
    } else if let Some(last_slash) = full_path.rfind('/') {
        let parent =
            if last_slash == 0 { "/".to_string() } else { full_path[..last_slash].to_string() };
        let name = full_path[last_slash + 1..].to_string();
        (parent, name)
    } else {
        (current_path, full_path.clone())
    };

    if file_name.is_empty() {
        println!("touch: invalid filename");
        return;
    }

    // Lock filesystem and perform operations
    let mut fs = fs_mutex().lock();

    // Open parent directory
    let parent_dir = match fs.open(&parent_path) {
        Ok(dir) => match fs.is_directory(&dir) {
            Ok(true) => dir,
            Ok(false) => {
                println!("touch: not a directory: {}", parent_path);
                return;
            }
            Err(e) => {
                println!("touch: error checking directory: {:?}", e);
                return;
            }
        },
        Err(e) => {
            println!("touch: cannot access parent directory '{}': {:?}", parent_path, e);
            return;
        }
    };

    // Check if file already exists
    match fs.find_in_directory(&parent_dir, &file_name) {
        Ok(_) => {
            // File exists, touch succeeded (would update timestamp in full implementation)
            return;
        }
        Err(_) => {
            // Expected - doesn't exist, we can create it
        }
    }

    // Create new empty file
    let options = CreateOptions {
        is_directory: false,
        data: vec![], // Empty file
    };

    match fs.create_file(&parent_dir, &file_name, options) {
        Ok(_) => {
            println!("Created file: {}", full_path);
        }
        Err(e) => {
            println!("touch: failed to create file: {:?}", e);
        }
    }
}

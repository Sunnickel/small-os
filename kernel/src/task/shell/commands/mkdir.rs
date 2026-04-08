use alloc::{
    string::{String, ToString},
    vec,
};

use driver::fs::{fs_mutex, is_initialized, CreateOptions};
use macros::command;

use crate::task::shell::{current_dir_path, resolve_path};

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

    // Check filesystem is initialized
    if !is_initialized() {
        println!("mkdir: filesystem not initialized");
        return;
    }

    let current_path = current_dir_path();
    let full_path = resolve_path(&dirname);

    // Get parent directory path and new directory name
    let (parent_path, dir_name) = if full_path == "/" {
        println!("mkdir: cannot create root directory");
        return;
    } else if let Some(last_slash) = full_path.rfind('/') {
        let parent =
            if last_slash == 0 { "/".to_string() } else { full_path[..last_slash].to_string() };
        let name = full_path[last_slash + 1..].to_string();
        (parent, name)
    } else {
        (current_path, full_path.clone())
    };

    if dir_name.is_empty() {
        println!("mkdir: invalid directory name");
        return;
    }

    // Lock filesystem and perform operations
    let mut fs = fs_mutex().lock();

    // Open parent directory
    let parent_dir = match fs.open(&parent_path) {
        Ok(dir) => match fs.is_directory(&dir) {
            Ok(true) => dir,
            Ok(false) => {
                println!("mkdir: not a directory: {}", parent_path);
                return;
            }
            Err(e) => {
                println!("mkdir: error checking directory: {:?}", e);
                return;
            }
        },
        Err(e) => {
            println!("mkdir: cannot access parent directory '{}': {:?}", parent_path, e);
            return;
        }
    };

    // Check if target already exists
    match fs.find_in_directory(&parent_dir, &dir_name) {
        Ok(_) => {
            println!("mkdir: already exists: {}/{}", parent_path, dir_name);
            return;
        }
        Err(_) => {
            // Expected - doesn't exist, we can create it
        }
    }

    // Create directory with proper options
    let options = CreateOptions {
        is_directory: true,
        data: vec![], // Directories don't need data
    };

    match fs.create_file(&parent_dir, &dir_name, options) {
        Ok(_) => {
            println!("Created directory: {}", full_path);
        }
        Err(e) => {
            println!("mkdir: failed to create directory: {:?}", e);
        }
    }
}

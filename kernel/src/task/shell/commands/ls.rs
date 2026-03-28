use alloc::format;
// commands/ls.rs (detailed version)
use alloc::string::String;

use driver::fs::{fs_mutex, is_initialized};
use macros::commands::command;

use crate::task::shell::{current_dir_path, resolve_path};

#[command(
    name = "ls",
    short = "List directory contents",
    long = "List files and directories. Usage: ls [path]"
)]
async fn ls(path: Option<String>) {
    // Check filesystem is initialized
    if !is_initialized() {
        println!("ls: filesystem not initialized");
        return;
    }

    // Determine target path
    let target_path = match path.as_deref() {
        None | Some("") => current_dir_path(),
        Some(p) => resolve_path(p),
    };

    // Lock filesystem and perform operations
    let mut fs = fs_mutex().lock();

    // Open target directory
    let dir = match fs.open(&target_path) {
        Ok(file) => {
            match fs.is_directory(&file) {
                Ok(true) => file,
                Ok(false) => {
                    // If it's a file, just show its name
                    match fs.file_name(&file) {
                        Ok(Some(name)) => {
                            println!("{}", name);
                            return;
                        }
                        Ok(None) => {
                            println!("(unknown file)");
                            return;
                        }
                        Err(e) => {
                            println!("ls: error: {:?}", e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    println!("ls: error checking directory: {:?}", e);
                    return;
                }
            }
        }
        Err(e) => {
            println!("ls: cannot access '{}': {:?}", target_path, e);
            return;
        }
    };

    // List directory contents
    match fs.list_directory(&dir) {
        Ok(entries) => {
            if entries.is_empty() {
                println!("(empty directory)");
            } else {
                for entry in entries {
                    // Try to determine if entry is a directory by checking if we can open it as one
                    let entry_path = if target_path == "/" {
                        format!("/{}", entry)
                    } else {
                        format!("{}/{}", target_path, entry)
                    };

                    let prefix = match fs.open(&entry_path) {
                        Ok(child) => match fs.is_directory(&child) {
                            Ok(true) => "d ",
                            Ok(false) => "- ",
                            Err(_) => "? ",
                        },
                        Err(_) => "? ",
                    };

                    println!("{}{}", prefix, entry);
                }
            }
        }
        Err(e) => {
            println!("ls: failed to list directory: {:?}", e);
        }
    }
}

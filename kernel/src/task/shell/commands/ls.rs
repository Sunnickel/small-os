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
    if !is_initialized() {
        println!("ls: filesystem not initialized");
        return;
    }

    let target_path = match path.as_deref() {
        None | Some("") => current_dir_path(),
        Some(p) => resolve_path(p),
    };

    // Collect everything we need while holding the lock
    let entries = {
        let mut fs = fs_mutex().lock();

        let dir = match fs.open(&target_path) {
            Ok(f) => f,
            Err(e) => {
                println!("ls: cannot access '{}': {:?}", target_path, e);
                return;
            }
        };

        match fs.is_directory(&dir) {
            Ok(false) => match fs.file_name(&dir) {
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
            },
            Err(e) => {
                println!("ls: error checking directory: {:?}", e);
                return;
            }
            Ok(true) => {}
        }

        match fs.list_directory(&dir) {
            Ok(e) => e,
            Err(e) => {
                println!("ls: failed to list directory: {:?}", e);
                return;
            }
        }
        // lock drops here
    };

    if entries.is_empty() {
        println!("(empty directory)");
        return;
    }

    // Re-lock per entry to check type
    for entry in &entries {
        let entry_path = if target_path == "/" {
            format!("/{}", entry)
        } else {
            format!("{}/{}", target_path, entry)
        };

        let prefix = {
            let mut fs = fs_mutex().lock();
            match fs.open(&entry_path) {
                Ok(child) => match fs.is_directory(&child) {
                    Ok(true) => "d ",
                    Ok(false) => "- ",
                    Err(_) => "? ",
                },
                Err(_) => "? ",
            }
            // lock drops here
        };

        println!("{}{}", prefix, entry);
    }
}

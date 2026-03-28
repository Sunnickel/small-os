use alloc::string::String;
use macros::commands::command;
use crate::task::shell::{list_current_dir, current_dir_path};

#[command(
    name = "ls",
    short = "List Directories",
    long = "Lists the directories in your current dir"
)]
async fn list() {
    match list_current_dir() {
        Ok(entries) => {
            if entries.is_empty() {
                println!("(empty directory)");
            } else {
                for entry in entries {
                    println!("{}", entry);
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}



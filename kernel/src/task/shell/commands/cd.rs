use alloc::string::String;

use macros::command;

#[command(
    name = "cd",
    short = "Change directory",
    long = "Change current working directory. Usage: cd <path>"
)]
async fn cd(path: String) {
    use crate::task::shell::change_dir;

    if let Err(e) = change_dir(&path) {
        println!("cd: {}", e);
    }
}

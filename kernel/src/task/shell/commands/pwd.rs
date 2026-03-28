use macros::commands::command;
use crate::task::shell::current_dir_path;

#[command(
    name = "pwd",
    short = "Print working directory",
    long = "Print current directory path"
)]
async fn pwd() {
    println!("{}", current_dir_path());
}
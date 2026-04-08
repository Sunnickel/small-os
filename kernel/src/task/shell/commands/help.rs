use macros::command;

use crate::task::shell::commands;

#[command(name = "help", short = "List commands", long = "Lists all available shell commands")]
async fn cmd_help() {
    let map = commands::COMMANDS.lock();
    for name in map.keys() {
        println!("  {}", name);
    }
}

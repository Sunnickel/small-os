use crate::println;
use crate::task::shell::commands;
use kernel_commands_macro::command;

#[command("help")]
async fn cmd_help(_args: &[&str]) {
    let map = commands::COMMANDS.lock();
    for name in map.keys() {
        println!("  {}", name);
    }
}

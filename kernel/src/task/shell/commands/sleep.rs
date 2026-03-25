use kernel_commands_macro::command;
use crate::println;

#[command("echo")]
async fn cmd_echo(args: &[&str]) {
    println!("{}", args.join(" "));
}
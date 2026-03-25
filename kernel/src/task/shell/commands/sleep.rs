use kernel_commands_macro::command;
use crate::task::sleep::Sleep;

#[command("sleep")]
async fn cmd_echo(args: &[&str]) {
    if let Ok(ticks) = args.join("")[6..].parse::<usize>() {
        Sleep::new(ticks).await;
    }
}
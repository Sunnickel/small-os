use macros::command;

use crate::task::sleep::Sleep;

#[command(
    name = "sleep",
    short = "Sleep for N ticks",
    long = "Pauses the shell for the given number of ticks"
)]
async fn cmd_sleep(ticks: Option<usize>) {
    if let Some(t) = ticks {
        Sleep::new(t).await;
    }
}

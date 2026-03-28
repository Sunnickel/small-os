use alloc::string::String;

use macros::commands::command;

#[command(name = "echo", short = "Print text", long = "Prints the given text to the screen")]
async fn cmd_echo(text: String, #[flag] upper: bool) {
    if upper {
        println!("{}", text.to_uppercase());
    } else {
        println!("{}", text);
    }
}

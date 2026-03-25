mod scancodes;
pub mod commands;

use crate::screen::SCREEN_WRITER;
use crate::task::sleep::Sleep;
use crate::{print, println};
use core::fmt::Write;
use futures_util::stream::StreamExt;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use crate::task::shell::scancodes::ScancodeStream;

pub async fn shell_task() {
    let mut stream = ScancodeStream::new();
    let mut line_buffer = heapless::String::<128>::new();

    let mut keyboard = Keyboard::new(
        ScancodeSet1::default(),
        layouts::De105Key,
        HandleControl::Ignore,
    );

    print!("> ");

    loop {
        if let Some(scancode) = stream.next().await {
            if let Ok(Some(event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(event) {
                    match key {
                        DecodedKey::Unicode(c) => match c {
                            '\n' => {
                                print!("\n");
                                execute_command(&line_buffer).await;
                                line_buffer.clear();
                                print!("> ");
                            }
                            '\x08' => {
                                // backspace
                                line_buffer.pop();
                                erase_last_char();
                            }
                            _ => {
                                if line_buffer.push(c).is_ok() {
                                    print!("{}", c);
                                }
                            }
                        },
                        DecodedKey::RawKey(_) => {}
                    }
                }
            }
        }
    }
}

async fn execute_command(line: &str) {
    match line {
        "help" => println!("Commands: help, echo, sleep"),
        s if s.starts_with("echo ") => println!("{}", &s[5..]),
        s if s.starts_with("sleep ") => {
            if let Ok(ticks) = s[6..].parse::<usize>() {
                Sleep::new(ticks).await;
            }
        }
        _ => println!("Unknown command"),
    }
}

fn erase_last_char() {
    print!("\x08 \x08");
}

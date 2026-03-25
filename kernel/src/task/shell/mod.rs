mod scancodes;
pub mod commands;

use crate::task::sleep::Sleep;
use crate::{print, println};
use futures_util::stream::StreamExt;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use crate::task::shell::scancodes::ScancodeStream;

pub async fn shell_task() {
    unsafe {
        commands::init_commands();
    }

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
    let mut parts = line.split_whitespace();
    let name = match parts.next() {
        Some(n) => n,
        None => return,
    };

    let args: heapless::Vec<&str, 16> = parts.collect();

    let func = {
        let map = commands::COMMANDS.lock();
        map.get(name).map(|e| e.func)
    };

    match func {
        Some(f) => f(args.as_slice()).await,
        None => println!("Unknown command: {}", name),
    }
}

fn erase_last_char() {
    print!("\x08 \x08");
}

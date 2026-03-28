pub mod commands;
pub mod history;
mod scancodes;
pub mod tokenizer;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::task::shell::history::History;
use crate::task::shell::scancodes::ScancodeStream;
use crate::task::shell::tokenizer::parser::{execute_pipeline, parse_pipeline};
use crate::task::shell::tokenizer::tokenize;
use futures_util::stream::StreamExt;
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1, layouts};
use spin::{Mutex, Once};
use driver::fs::{root_directory, open, list_directory, NtfsFile};

// Track current path as string - NtfsFile is a snapshot, not live data
static CURRENT_PATH: Once<Mutex<String>> = Once::new();

fn current_path_mutex() -> &'static Mutex<String> {
    CURRENT_PATH.get().expect("current directory not initialized")
}

fn get_current_path() -> String {
    current_path_mutex().lock().clone()
}

fn set_current_path(path: &str) {
    *current_path_mutex().lock() = path.to_string();
}

fn get_prompt() -> String {
    format!("{}> ", get_current_path())
}

pub async fn shell_task() {
    unsafe {
        commands::init_commands();
    }

    // Initialize with root path
    CURRENT_PATH.call_once(|| Mutex::new("/".to_string()));

    let prefix = get_prompt();
    let mut stream = ScancodeStream::new();
    let mut line = heapless::String::<128>::new();
    let mut history = History::new();

    let mut keyboard = Keyboard::new(
        ScancodeSet1::default(),
        layouts::De105Key,
        HandleControl::Ignore,
    );

    print!("{}", prefix);

    loop {
        if let Some(scancode) = stream.next().await {
            if let Ok(Some(event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(event) {
                    match key {
                        DecodedKey::Unicode(c) => match c {
                            '\n' => {
                                let history_line = line.clone();
                                let token_line = line.clone().replace(prefix.as_str(), "\n");

                                println!();

                                history.push(&*history_line);

                                let tokens = tokenize(&*token_line);
                                let cmds = parse_pipeline(&tokens);

                                execute_pipeline(cmds).await;

                                line.clear();
                                print!("{}", get_prompt());
                            }
                            c => {
                                if line.push(c).is_ok() {
                                    print!("{}", c);
                                }
                            }
                        },
                        DecodedKey::RawKey(KeyCode::Backspace) => {
                            if line.pop().is_some() {
                                print!("\x08 \x08");
                            }
                        }
                        DecodedKey::RawKey(KeyCode::ArrowUp) => {
                            if let Some(prev) = history.up() {
                                clear_line(&line);
                                line.clear();
                                line.push_str(prev).ok();
                                print!("{}", line);
                            }
                        }
                        DecodedKey::RawKey(KeyCode::ArrowDown) => {
                            if let Some(next) = history.down() {
                                clear_line(&line);
                                line.clear();
                                line.push_str(next).ok();
                                print!("{}", line);
                            }
                        }
                        DecodedKey::RawKey(_) => {}
                    }
                }
            }
        }
    }
}

fn clear_line(line: &str) {
    for _ in 0..line.len() {
        print!("\x08 \x08");
    }
}

/// Resolve a path (relative or absolute) to full path
fn resolve_path(path: &str) -> String {
    if path.starts_with('/') {
        // Absolute path
        path.to_string()
    } else if path == "." {
        // Current directory
        get_current_path()
    } else if path == ".." {
        // Parent directory
        let current = get_current_path();
        if current == "/" {
            "/".to_string()
        } else {
            let parts: Vec<&str> = current.trim_end_matches('/').split('/').collect();
            if parts.len() <= 1 {
                "/".to_string()
            } else {
                parts[..parts.len()-1].join("/")
            }
        }
    } else {
        // Relative path
        let current = get_current_path();
        if current == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", current, path)
        }
    }
}

/// Open current directory fresh from filesystem
pub fn open_current_dir() -> Result<NtfsFile, &'static str> {
    let path = get_current_path();
    open(&path).map_err(|_| "failed to open current directory")
}

/// List current directory contents (fresh read)
pub fn list_current_dir() -> Result<Vec<String>, &'static str> {
    let dir = open_current_dir()?;
    if !dir.is_directory() {
        return Err("not a directory");
    }
    list_directory(&dir).map_err(|_| "failed to list directory")
}

/// Change current directory
pub fn change_dir(path: &str) -> Result<(), &'static str> {
    let new_path = resolve_path(path);

    // Verify it exists and is a directory
    match open(&new_path) {
        Ok(dir) => {
            println!("is_directory = {}", dir.is_directory());
            if dir.is_directory() {
                set_current_path(&new_path);
                Ok(())
            } else {
                Err("not a directory")
            }
        }
        Err(_) => Err("no such directory"),
    }
}

/// Get current path for commands
pub fn current_dir_path() -> String {
    get_current_path()
}
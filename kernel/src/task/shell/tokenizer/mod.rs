pub mod parser;

use alloc::boxed::Box;
use alloc::string::String;

pub enum Token<'a> {
    Word(&'a str),
    Pipe,
}

pub fn tokenize(input: &'_ str) -> heapless::Vec<Token<'_>, 64> {
    let mut tokens = heapless::Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' => {
                chars.next();
            }
            '|' => {
                chars.next();
                tokens.push(Token::Pipe).ok();
            }
            '"' => {
                chars.next();
                let mut start = String::new();
                while let Some(&c2) = chars.peek() {
                    if c2 == '"' {
                        break;
                    }
                    start.push(c2);
                    chars.next();
                }
                chars.next();
                tokens
                    .push(Token::Word(Box::leak(start.into_boxed_str())))
                    .ok();
            }
            _ => {
                let mut word = String::new();
                while let Some(&c2) = chars.peek() {
                    if c2 == ' ' || c2 == '|' {
                        break;
                    }
                    word.push(c2);
                    chars.next();
                }
                tokens
                    .push(Token::Word(Box::leak(word.into_boxed_str())))
                    .ok();
            }
        }
    }

    tokens
}

use crate::task::shell::commands::COMMANDS;
use crate::task::shell::tokenizer::Token;

pub struct Command<'a> {
    pub name: &'a str,
    pub args: heapless::Vec<&'a str, 16>,
}

pub fn parse_pipeline<'a>(tokens: &[Token<'a>]) -> heapless::Vec<Command<'a>, 8> {
    let mut cmds = heapless::Vec::new();
    let mut current = Command {
        name: "",
        args: heapless::Vec::new(),
    };

    for token in tokens {
        match token {
            Token::Word(w) => {
                if current.name.is_empty() {
                    current.name = w;
                } else {
                    current.args.push(w).ok();
                }
            }
            Token::Pipe => {
                cmds.push(current).ok();
                current = Command {
                    name: "",
                    args: heapless::Vec::new(),
                };
            }
        }
    }

    if !current.name.is_empty() {
        cmds.push(current).ok();
    }

    cmds
}

pub async fn execute_pipeline(cmds: heapless::Vec<Command<'_>, 8>) {
    for cmd in cmds {
        let future = {
            let map = COMMANDS.lock();
            if let Some(entry) = map.get(cmd.name) {
                Some((entry.func)(&cmd.args))
            } else {
                None
            }
        };

        match future {
            Some(f) => f.await,
            None => println!("Unknown command: {}", cmd.name),
        }
    }
}

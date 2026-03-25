use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::pin::Pin;
use spin::Mutex;

pub mod echo;
pub mod sleep;

pub type CommandFuture<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

pub struct CommandEntry {
    pub name: &'static str,
    pub func: for<'a> fn(&'a [&'a str]) -> CommandFuture<'a>,
}

pub type CommandFn = fn(&[&str]) -> ();

lazy_static::lazy_static! {
    #[link_section = ".commands"]
    pub static ref COMMANDS:
        Mutex<BTreeMap<&'static str, CommandEntry>> =
        Mutex::new(BTreeMap::new());
}

unsafe extern "C" {
    static __start_commands: CommandEntry;
    static __stop_commands: CommandEntry;
}

pub fn init_commands() {
    let start = unsafe { &__start_commands as *const CommandEntry };
    let end = unsafe { &__stop_commands as *const CommandEntry };

    let mut current = start;

    let mut map = COMMANDS.lock();

    while current < end {
        let entry = unsafe { &*current };
        map.insert(entry.name, *entry);

        current = unsafe { current.add(1) };
    }
}
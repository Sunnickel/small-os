use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::pin::Pin;
use spin::Mutex;

pub mod echo;
pub mod sleep;
pub mod help;

pub type CommandFuture<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CommandEntry {
    pub name: &'static str,
    pub func: for<'a> fn(&'a [&'a str]) -> CommandFuture<'a>,
}

pub type CommandFn = fn(&[&str]) -> ();

lazy_static::lazy_static! {
    pub static ref COMMANDS:
        Mutex<BTreeMap<&'static str, CommandEntry>> =
        Mutex::new(BTreeMap::new());
}

#[allow(improper_ctypes)]
unsafe extern "C" {
    static __start_commands: CommandEntry;
    static __stop_commands: CommandEntry;
}

pub unsafe fn init_commands() {
    let start = core::ptr::addr_of!(__start_commands);
    let end   = core::ptr::addr_of!(__stop_commands);

    if start == end {
        return;
    }

    let count = unsafe {
        end.offset_from(start) as usize
    };

    let mut map = COMMANDS.lock();

    for i in 0..count {
        let entry = unsafe { &*start.add(i) };
        map.insert(entry.name, *entry);
    }
}
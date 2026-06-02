use crate::sync::spinlock::SpinLock;

static CMDLINE: SpinLock<Option<&'static str>> = SpinLock::new(None);

pub fn init(cmdline: Option<&'static str>) {
    *CMDLINE.lock() = cmdline;
}

pub fn command_line() -> Option<&'static str> {
    *CMDLINE.lock()
}

pub fn contains(flag: &str) -> bool {
    command_line()
        .map(|cmdline| cmdline.split_whitespace().any(|part| part == flag))
        .unwrap_or(false)
}

pub fn value(name: &str) -> Option<&'static str> {
    command_line()?.split_whitespace().find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name).then_some(value)
    })
}

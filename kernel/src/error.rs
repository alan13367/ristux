use core::fmt;

pub type KernelResult<T> = Result<T, KernelError>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum KernelError {
    SelfTestFailed(&'static str),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfTestFailed(message) => f.write_str(message),
        }
    }
}

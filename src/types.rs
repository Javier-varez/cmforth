//! Data types used by the forth interpreter. These depend on the target architecture, as native
//! word sizes are used in Forth.

/// Native word type of the target machine.
#[cfg(target_arch = "arm")]
pub type Word = u32;

/// Native address type of the target machine.
#[cfg(target_arch = "arm")]
pub type Address = Word;

/// Native word type of the target machine.
#[cfg(target_arch = "x86_64")]
pub type Word = u64;

/// Native address type of the target machine.
#[cfg(target_arch = "x86_64")]
pub type Address = Word;

//! Data types used by the forth interpreter. These depend on the target architecture, as native
//! word sizes are used in Forth.

/// Native word type of the target machine. Currently only defined for Cortex-M cores.
pub type Word = u32;

/// Native address type of the target machine. Currently only defined for Cortex-M cores.
pub type Address = Word;

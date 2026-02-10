use crate::{
    error::Error,
    io::{IoWriter, ReaderWriter},
    stack::{Stack, StackProperties},
    types::{Address, Word},
};
use core::fmt::Write;

#[cfg(all(target_os = "none", target_arch = "arm", feature = "cortex-m-arch"))]
mod thumbv7m;

#[cfg(all(target_os = "none", target_arch = "arm", feature = "cortex-m-arch"))]
use thumbv7m::*;

#[cfg(all(
    not(all(target_os = "none", target_arch = "arm")),
    feature = "cortex-m-arch"
))]
compile_error!("The cortex-m-arch feature can only be used for ARM none EABI targets.");

#[cfg(feature = "std")]
mod sw;

#[cfg(feature = "std")]
use sw::*;

#[derive(Default, Debug)]
#[repr(u32)]
pub enum ExitReason {
    #[default]
    Success = 0,
    DotOp,
    WordOp,
    FindOp,
    KeyOp,
    TellOp,
    EmitOp,
}

#[derive(Default, Debug)]
#[repr(u32)] // TODO: Different repr based on arch
pub enum State {
    #[default]
    ImmediateMode = 0,

    // This is actually set by asm code
    #[allow(dead_code)]
    CompilationMode = 1,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct FlagsAndLength(u8);

impl FlagsAndLength {
    const F_IMMED: u8 = 0x80;
    const F_HIDDEN: u8 = 0x20;
    const F_LENMASK: u8 = 0x1f;

    pub fn length(&self) -> usize {
        (self.0 & Self::F_LENMASK) as usize
    }

    pub fn is_immediate(&self) -> bool {
        (self.0 & Self::F_IMMED) != 0
    }

    pub fn is_hidden(&self) -> bool {
        (self.0 & Self::F_HIDDEN) != 0
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct WordHeader {
    next: Address,
    flags_and_length: FlagsAndLength,
}

struct WordDef {
    location: Address,
}

impl WordDef {
    pub fn at(location: Address) -> Self {
        Self { location }
    }

    pub fn header(&self) -> WordHeader {
        unsafe { *(self.location as *const WordHeader) }
    }

    pub fn name(&self) -> Result<&str, Error> {
        let header = unsafe { *(self.location as *const WordHeader) };
        let word_base = self.location as usize
            + core::mem::size_of::<Address>()
            + core::mem::size_of::<FlagsAndLength>();
        let word_data = unsafe {
            core::slice::from_raw_parts(word_base as *const u8, header.flags_and_length.length())
        };
        core::str::from_utf8(word_data).map_err(|_| Error::CorruptWordDef(self.location))
    }

    pub fn cfa(&self) -> Result<Address, Error> {
        let header = unsafe { *(self.location as *const WordHeader) };
        let cfa_unaligned = self.location
            + core::mem::size_of::<Address>() as Address
            + core::mem::size_of::<FlagsAndLength>() as Address
            + header.flags_and_length.length() as Address;
        const ALIGNMENT: Address = core::mem::size_of::<Word>() as Address;
        Ok(if !cfa_unaligned.is_multiple_of(ALIGNMENT) {
            (cfa_unaligned + ALIGNMENT - 1) & !(ALIGNMENT - 1)
        } else {
            cfa_unaligned
        })
    }
}

#[derive(Debug, Clone)]
#[repr(i32)]
enum FindStatus {
    NotFound = 0,
    Immediate = 1,
    NotImmediate = -1,
}

#[derive(Default)]
#[repr(C)]
pub struct ForthVariables {
    pub state: State,
    pub latest: Address, // Pointer to program data or compile_area
    pub base: Word,      // Numeric base in use.
}

#[repr(C)]
pub struct ForthContext<'a> {
    pub dsp: StackProperties<'a, Word>,       // sp
    pub rsp: StackProperties<'a, Address>,    // r5
    pub cpa: StackProperties<'a, Word, true>, // r5
    pub ip: Address,                          // r4
    pub lr: Address,
    pub variables: ForthVariables,
    pub host_ctx: HostSaveContext,
}

impl<'a> ForthContext<'a> {
    pub fn new(
        mut data_stack_area: Stack<'a, Word>,
        mut return_stack_area: Stack<'a, Address>,
        mut compile_area: Stack<'a, Word, true>,
    ) -> Self {
        let dsp = data_stack_area.properties();
        let rsp = return_stack_area.properties();
        let cpa = compile_area.properties();
        let ip = 0; // To be filled in by the interpreter
        let lr = 0; // To be filled in by the interpreter
        let variables = ForthVariables {
            state: State::ImmediateMode,
            latest: initial_latest(),
            base: 10,
        };
        let host_ctx = HostSaveContext::new();

        Self {
            dsp,
            rsp,
            cpa,
            ip,
            lr,
            variables,
            host_ctx,
        }
    }

    /// Returns the base address of the requested word, if found. Note this is the base address in
    /// the linked list, not the CFA or DFA.
    unsafe fn search_word(latest: Address, name: &str) -> Result<WordDef, Error> {
        let mut cur_word = WordDef::at(latest);
        loop {
            if cur_word.location == 0 {
                return Err(Error::WordNotFound);
            }

            if cur_word.name()? == name && !cur_word.header().flags_and_length.is_hidden() {
                return Ok(cur_word);
            }

            cur_word = WordDef::at(cur_word.header().next);
        }
    }

    fn dot_op<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        let v = self.dsp.pop()?;
        let mut writer = IoWriter::new(io);
        write!(writer, "{v:x}").expect("IoWriter is guaranteed to never error");
        Ok(())
    }

    fn word_op<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        let word = io.read_word();
        let addr = word.as_ptr() as Address;
        let len = word.len() as Word;

        self.dsp.push(addr)?;
        self.dsp.push(len)?;
        Ok(())
    }

    unsafe fn find_op(&mut self) -> Result<(), Error> {
        let len = self.dsp.pop()?;
        let addr = self.dsp.pop()?;
        let word = unsafe { &*core::ptr::slice_from_raw_parts(addr as *const _, len as usize) };
        let word = core::str::from_utf8(word).map_err(|_| Error::InvalidWord)?;

        let word = unsafe { Self::search_word(self.variables.latest, word) };
        match word {
            Ok(word) => {
                self.dsp.push(word.location as Word)?;
                self.dsp
                    .push(if word.header().flags_and_length.is_immediate() {
                        FindStatus::Immediate
                    } else {
                        FindStatus::NotImmediate
                    } as Word)?;
            }
            Err(Error::WordNotFound) => {
                self.dsp.push(FindStatus::NotFound as Word)?;
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }

    fn key_op<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        let c = io.read();
        self.dsp.push(c as Word)?;
        Ok(())
    }

    unsafe fn tell_op<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        let len = self.dsp.pop()?;
        let addr = self.dsp.pop()?;

        let str = unsafe { &*core::ptr::slice_from_raw_parts(addr as *const _, len as usize) };
        let str = core::str::from_utf8(str).map_err(|_| Error::InvalidString)?;

        let mut writer = IoWriter::new(io);
        write!(writer, "{str}").expect("IoWriter is guaranteed to never error");

        Ok(())
    }

    fn emit_op<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        let c: char = self.dsp.pop()? as u8 as char;
        let mut writer = IoWriter::new(io);
        write!(writer, "{c}").expect("IoWriter is guaranteed to never error");

        Ok(())
    }

    unsafe fn exec_word<T: ReaderWriter>(
        &mut self,
        word_def: WordDef,
        io: &mut T,
    ) -> Result<(), Error> {
        // Helper impl to execute the target word and then exit the interpreter
        // to Rust code again
        let jump_word_def = [word_def.cfa()?, exit_fn()];
        let jump_word_dfa = &jump_word_def as *const _ as Address;
        self.ip = jump_word_dfa;
        self.lr = initial_lr();

        loop {
            let exit_reason = unsafe { enter_forth(self) };

            match exit_reason {
                ExitReason::Success => {
                    break;
                }
                ExitReason::DotOp => self.dot_op(io)?,
                ExitReason::WordOp => self.word_op(io)?,
                ExitReason::FindOp => unsafe { self.find_op()? },
                ExitReason::KeyOp => self.key_op(io)?,
                ExitReason::TellOp => unsafe { self.tell_op(io)? },
                ExitReason::EmitOp => self.emit_op(io)?,
            };
        }

        Ok(())
    }

    pub unsafe fn interpret_one<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        let word = io.read_word();
        if word.is_empty() {
            return Ok(());
        }
        let word = core::str::from_utf8(word).map_err(|_| Error::InvalidWord)?;
        log::debug!("Interpreting word {word}");

        let maybe_word = unsafe { Self::search_word(self.variables.latest, word) };

        match self.variables.state {
            State::ImmediateMode => match maybe_word {
                Ok(word) => unsafe { self.exec_word(word, io) },
                Err(Error::WordNotFound) => {
                    let num = Word::from_str_radix(word, self.variables.base as u32)
                        .map_err(|_| Error::WordNotFound)?;
                    self.dsp.push(num)
                }
                Err(e) => Err(e),
            },
            State::CompilationMode => match maybe_word {
                Ok(word) if word.header().flags_and_length.is_immediate() => unsafe {
                    self.exec_word(word, io)
                },
                Ok(word) => {
                    self.cpa.push(word.cfa()?)?;
                    Ok(())
                }
                Err(Error::WordNotFound) => {
                    let num: Word = Word::from_str_radix(word, self.variables.base as u32)
                        .map_err(|_| Error::WordNotFound)?;
                    self.cpa.push(forth_lit())?;
                    self.cpa.push(num)?;
                    Ok(())
                }
                Err(e) => Err(e),
            },
        }
    }

    /// # Safety
    ///   Forth can perform arbitrary memory reads/writes. Therefore, you must
    ///   guarantee that the forth program is correct and only alters data owned
    ///   by the interpreter. This data is:
    ///    - The data stack.
    ///    - The return stack.
    ///    - The compile area.
    ///
    ///   Additionally, since new programs can be compiled inside forth, you must
    ///   guarantee that the programs are well formed. They must:
    ///    - Include appropriate control operations, like returning from a forth
    ///      word or handling loops correctly.
    ///    - Ensure that jumps to forth words are all valid words.
    ///    - Any other constrain added by your forth program.
    pub unsafe fn execute<T: ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        // The outer interpreter is implemented in Rust. The outer interpreter does not
        // significantly affect performance of the Forth implementation because it
        // is responsible for handling user I/O, and, as such, is expected to be I/O-bound
        // rather than compute-bound.

        loop {
            unsafe { self.interpret_one(io)? };
        }
    }
}

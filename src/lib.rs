#![no_std]

pub mod error;
mod interpreter;
pub mod io;
pub mod stack;
pub mod types;

use error::Error;
use interpreter::ForthContext;
use stack::Stack;
use types::{Address, Word};

pub struct Forth<'a> {
    context: ForthContext<'a>,
}

impl<'a> Forth<'a> {
    pub fn new(
        data_stack: Stack<'a, Word>,
        return_stack: Stack<'a, Address>,
        compile_area: Stack<'a, Word, true>,
    ) -> Self {
        Self {
            context: ForthContext::new(data_stack, return_stack, compile_area),
        }
    }

    /// Executes the forth interpreter.
    ///
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
    pub unsafe fn run<T: io::ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        unsafe { self.context.execute(io)? };
        Ok(())
    }

    pub unsafe fn interpret_one<T: io::ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        unsafe { self.context.interpret_one(io)? };
        Ok(())
    }
}

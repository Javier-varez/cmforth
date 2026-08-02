#![no_std]
#![cfg_attr(test, no_main)]

pub mod error;
mod interpreter;
pub mod io;
pub mod stack;
pub mod types;

use error::Error;
use interpreter::ForthContext;
use stack::Stack;
use types::{Address, Word};

/// Additional forth sources
pub static FORTH_SOURCE: &str = include_str!("../forth.f");

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

    /// Interprets a single word using the forth interpreter.
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
    pub unsafe fn interpret_one<T: io::ReaderWriter>(&mut self, io: &mut T) -> Result<(), Error> {
        unsafe { self.context.interpret_one(io)? };
        Ok(())
    }

    /// Provides immutable access to the execution context of the forth interpreter.
    pub fn context(&self) -> &ForthContext<'a> {
        &self.context
    }

    /// Provides mutable access to the execution context of the forth interpreter.
    pub fn context_mut(&mut self) -> &mut ForthContext<'a> {
        &mut self.context
    }
}

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use super::*;
    use static_cell::StaticCell;

    use crate::{
        io::{CombinedIo, SemihostingIo, StringReader},
        stack::StackStorage,
    };

    // Force the linking of this library, which we need to start up the binary
    use cortex_m_rt as _;
    use defmt_rtt as _;

    const DATA_STACK_WORDS: usize = 128;
    const RETURN_STACK_ADDRESSES: usize = 128;
    const COMPILE_AREA_WORDS: usize = 128;

    static DATA_STACK_STORAGE: StaticCell<StackStorage<DATA_STACK_WORDS, Word>> = StaticCell::new();
    static RETURN_STACK_STORAGE: StaticCell<StackStorage<RETURN_STACK_ADDRESSES, Address>> =
        StaticCell::new();
    static COMPILE_AREA_STORAGE: StaticCell<StackStorage<COMPILE_AREA_WORDS, Word>> =
        StaticCell::new();

    struct Context {
        forth: Forth<'static>,
    }

    fn build_io(input: &str) -> CombinedIo<StringReader<'_>, SemihostingIo> {
        let reader = StringReader::new(input);
        let writer = SemihostingIo::new();
        CombinedIo::new(reader, writer)
    }

    fn run_program(ctx: &mut Context, input: &str) {
        let mut io = build_io(input);
        while !io.reader.is_eof() {
            unsafe { ctx.forth.interpret_one(&mut io).unwrap() };
        }
    }

    fn run_program_assert_stack(ctx: &mut Context, input: &str, expected_stack: &[Word]) {
        let mut io = build_io(input);
        while !io.reader.is_eof() {
            let result = unsafe { ctx.forth.interpret_one(&mut io) };
            defmt::info!("result: {}", result);
            assert_eq!(result, Ok(()));
        }

        let stack = &mut ctx.forth.context_mut().dsp;
        for word in expected_stack {
            let result = stack.pop();
            defmt::info!("{} vs {}", result, *word);
            assert_eq!(result, Ok(*word));
        }
        let result = stack.pop();
        assert_eq!(result, Err(Error::StackUnderflow));
    }

    fn run_program_expect_error(ctx: &mut Context, input: &str, expected_error: Error) {
        let mut io = build_io(input);
        let mut result = Ok(());
        while !io.reader.is_eof() {
            result = unsafe { ctx.forth.interpret_one(&mut io) };
            defmt::info!("result: {}", result);
            if result.is_err() {
                break;
            }
        }

        assert_eq!(result, Err(expected_error));
        assert!(io.reader.is_eof());
    }

    #[init]
    fn init() -> Context {
        let data_stack_storage = DATA_STACK_STORAGE.init_with(StackStorage::new);
        let return_stack_storage = RETURN_STACK_STORAGE.init_with(StackStorage::new);
        let compile_area_storage = COMPILE_AREA_STORAGE.init_with(StackStorage::new);

        let forth = Forth::new(
            Stack::new_with(data_stack_storage),
            Stack::new_with(return_stack_storage),
            Stack::new_with(compile_area_storage),
        );

        Context { forth }
    }

    #[test]
    fn num_literals(mut ctx: Context) {
        run_program_assert_stack(&mut ctx, "23 34", &[34, 23]);
    }

    #[test]
    fn dup_add(mut ctx: Context) {
        run_program_assert_stack(&mut ctx, "23 DUP + DUP +", &[23 * 4]);
    }

    #[test]
    fn fn_add(mut ctx: Context) {
        run_program_assert_stack(&mut ctx, ": ADD + ; 23 43 ADD", &[23 + 43]);
    }

    #[test]
    fn immediate_words(mut ctx: Context) {
        run_program_assert_stack(
            &mut ctx,
            ": [ADD] IMMEDIATE + ; 23 43 : NEWWORD [ADD] ;",
            &[23 + 43],
        );
        run_program_assert_stack(&mut ctx, "23 43 : NEWWORD [ + ] ;", &[23 + 43]);
        run_program_assert_stack(&mut ctx, "23 43 : NEWWORD + ;", &[43, 23]);
        run_program_assert_stack(&mut ctx, "23 43 : NEWWORD [ + ] 34 ;", &[23 + 43]);
        run_program_assert_stack(&mut ctx, "23 43 : NEWWORD [ +  34 ] ;", &[34, 23 + 43]);
    }

    #[test]
    fn mem_read(mut ctx: Context) {
        let var = 123;
        let dsp = &mut ctx.forth.context_mut().dsp;
        dsp.push(&var as *const _ as Address).unwrap();
        run_program_assert_stack(&mut ctx, "@", &[123]);
    }

    #[test]
    fn mem_store(mut ctx: Context) {
        let mut var = 123;
        let dsp = &mut ctx.forth.context_mut().dsp;
        dsp.push(12).unwrap();
        dsp.push(&mut var as *mut _ as Address).unwrap();
        run_program_assert_stack(&mut ctx, "!", &[]);
        assert_eq!(var, 12);
    }

    #[test]
    fn drop(mut ctx: Context) {
        run_program_assert_stack(&mut ctx, "12 DROP", &[]);
    }

    #[test]
    fn drop_none(mut ctx: Context) {
        assert_eq!(ctx.forth.context().dsp.top, ctx.forth.context().dsp.ptr);
        run_program_expect_error(&mut ctx, "DROP", Error::StackUnderflow);
        assert_eq!(ctx.forth.context().dsp.top, ctx.forth.context().dsp.ptr);
        run_program_expect_error(&mut ctx, "DROP DROP", Error::StackUnderflow);
        assert_eq!(ctx.forth.context().dsp.top, ctx.forth.context().dsp.ptr);
    }

    #[test]
    fn fn_def_latest(mut ctx: Context) {
        let bottom = ctx.forth.context().cpa.bottom;
        run_program_assert_stack(&mut ctx, ": NEWWORD ; LATEST @", &[bottom]);
    }

    #[test]
    fn initial_latest_value(ctx: Context) {
        let forth_ctx = ctx.forth.context();
        assert_eq!(forth_ctx.variables.latest, interpreter::initial_latest());
    }

    #[test]
    fn compile_word(mut ctx: Context) {
        run_program(&mut ctx, ": TEST ;");

        let forth_ctx = ctx.forth.context_mut();
        assert_eq!(forth_ctx.dsp.pop(), Err(crate::Error::StackUnderflow));

        // TEST should be placed at the bottom of the compile area
        assert_eq!(forth_ctx.variables.latest, forth_ctx.cpa.bottom);

        let prev_word = forth_ctx.cpa.get(forth_ctx.variables.latest);
        assert_eq!(prev_word, Some(interpreter::initial_latest()));

        let name = forth_ctx
            .cpa
            .get(forth_ctx.variables.latest + core::mem::size_of::<Word>() as Address);
        assert_eq!(name, Some(Word::from_le_bytes(*b"\x04TES")));

        let name2 = forth_ctx
            .cpa
            .get(forth_ctx.variables.latest + 2 * core::mem::size_of::<Word>() as Address);
        assert_eq!(name2, Some(Word::from_le_bytes(*b"T\x00\x00\x00")));
    }
}

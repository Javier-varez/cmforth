#![no_std]
#![no_main]

use cortex_m_rt::entry;
use static_cell::StaticCell;

use cmforth::{
    Forth,
    io::{CombinedIo, SemihostingIo, StringReader},
    stack::{Stack, StackStorage},
    types::{Address, Word},
};

const DATA_STACK_WORDS: usize = 512;
const RETURN_STACK_ADDRESSES: usize = 128;
const COMPILE_AREA_WORDS: usize = 2048;

static DATA_STACK_STORAGE: StaticCell<StackStorage<DATA_STACK_WORDS, Word>> = StaticCell::new();
static RETURN_STACK_STORAGE: StaticCell<StackStorage<RETURN_STACK_ADDRESSES, Address>> =
    StaticCell::new();
static COMPILE_AREA_STORAGE: StaticCell<StackStorage<COMPILE_AREA_WORDS, Word>> = StaticCell::new();

use core::fmt::Write;
use core::panic::PanicInfo;

struct SemihostingWriter {}

impl Write for SemihostingWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for char in s.chars() {
            const SYS_WRITEC: u32 = 0x03;
            let v = char as u8;
            unsafe { cortex_m::asm::semihosting_syscall(SYS_WRITEC, (&v as *const _) as u32) };
        }
        Ok(())
    }
}

struct SemihostingLogger {
    level: log::LevelFilter,
}

impl SemihostingLogger {
    fn register(&'static self) {
        log::set_logger(self).unwrap();
        log::set_max_level(self.level);
    }
}

impl log::Log for SemihostingLogger {
    fn log(&self, record: &log::Record) {
        if record.metadata().level() <= self.level {
            let _ = writeln!(
                &mut SemihostingWriter {},
                "{} - {}: {}",
                record.target(),
                record.level(),
                record.args()
            );
        }
    }

    fn flush(&self) {}

    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }
}

#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = writeln!(&mut SemihostingWriter {}, "{}", info.message());
    loop {}
}

static LOGGER: SemihostingLogger = SemihostingLogger {
    level: log::LevelFilter::Info,
};

static FORTH_SOURCE: &str = include_str!("../forth.f");

#[entry]
fn main() -> ! {
    LOGGER.register();

    let data_stack_storage = DATA_STACK_STORAGE.init_with(StackStorage::new);
    let return_stack_storage = RETURN_STACK_STORAGE.init_with(StackStorage::new);
    let compile_area_storage = COMPILE_AREA_STORAGE.init_with(StackStorage::new);
    let mut forth = Forth::new(
        Stack::new_with(data_stack_storage),
        Stack::new_with(return_stack_storage),
        Stack::new_with(compile_area_storage),
    );

    {
        let mut initial_io = CombinedIo::new(StringReader::new(FORTH_SOURCE), SemihostingIo::new());
        while !initial_io.reader.is_eof() {
            unsafe { forth.interpret_one(&mut initial_io) }.unwrap();
        }
    }

    let mut io = SemihostingIo::new();
    unsafe {
        forth.run(&mut io).unwrap();
    }

    unreachable!();
}

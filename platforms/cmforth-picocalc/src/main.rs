//! cmforth interpreter for the PicoCalc.
#![no_std]
#![no_main]

mod display;
mod keyboard;
mod teletype;

use cmforth::{
    Forth,
    io::{CombinedIo, StringReader, Writer},
    stack::{Stack, StackStorage},
    types::{Address, Word},
};
use core::fmt::Write;
use defmt::*;
use defmt_rtt as _;
use display::PicoCalcDisplay;
use embedded_graphics::{pixelcolor::Rgb888, prelude::*};
use embedded_hal::i2c::I2c;
use embedded_hal::spi::MODE_0;
use keyboard::PicoCalcKeyboard;
use panic_probe as _;
use rp235x_hal::clocks::init_clocks_and_plls;
use rp235x_hal::fugit::RateExtU32;
use rp235x_hal::gpio::{FunctionSpi, PinState};
use rp235x_hal::{self as hal, entry};
use rp235x_hal::{Clock, pac};
use static_cell::StaticCell;
use teletype::Teletype;

const DATA_STACK_WORDS: usize = 512;
const RETURN_STACK_ADDRESSES: usize = 128;
const COMPILE_AREA_WORDS: usize = 2048;

static DATA_STACK_STORAGE: StaticCell<StackStorage<DATA_STACK_WORDS, Word>> = StaticCell::new();
static RETURN_STACK_STORAGE: StaticCell<StackStorage<RETURN_STACK_ADDRESSES, Address>> =
    StaticCell::new();
static COMPILE_AREA_STORAGE: StaticCell<StackStorage<COMPILE_AREA_WORDS, Word>> = StaticCell::new();

/// Tell the Boot ROM about our application
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let sio = hal::Sio::new(pac.SIO);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let delay = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // The LCD backlight is controlled by the PicoCalc's STM32, independently
    // of the LCD controller. Set it explicitly instead of relying on its
    // retained/default value.
    let mut system_i2c = hal::I2C::i2c1(
        pac.I2C1,
        pins.gpio6.reconfigure(),
        pins.gpio7.reconfigure(),
        10_000u32.Hz(),
        &mut pac.RESETS,
        clocks.system_clock.freq(),
    );
    if system_i2c.write(0x1f_u8, &[0x85, 0xf0]).is_err() {
        warn!("Could not set LCD backlight");
    }
    let keyboard = PicoCalcKeyboard::new(system_i2c);

    // PicoCalc display: SPI1 SCK=GP10, MOSI=GP11, MISO=GP12, CS=GP13,
    // DC=GP14, RESET=GP15.
    let sck = pins.gpio10.into_function::<FunctionSpi>();
    let mosi = pins.gpio11.into_function::<FunctionSpi>();
    let miso = pins.gpio12.into_function::<FunctionSpi>();
    let cs = pins.gpio13.into_push_pull_output_in_state(PinState::High);
    let dc = pins.gpio14.into_push_pull_output_in_state(PinState::High);
    let reset = pins.gpio15.into_push_pull_output_in_state(PinState::High);

    let spi = hal::Spi::<_, _, _, 8>::new(pac.SPI1, (mosi, miso, sck)).init(
        &mut pac.RESETS,
        clocks.peripheral_clock.freq(),
        25_000_000u32.Hz(),
        MODE_0,
    );

    let mut display = PicoCalcDisplay::new(spi, cs, dc, reset, delay);
    display.init().unwrap();
    display.clear(Rgb888::BLACK).unwrap();

    let data_stack_storage = DATA_STACK_STORAGE.init_with(StackStorage::new);
    let return_stack_storage = RETURN_STACK_STORAGE.init_with(StackStorage::new);
    let compile_area_storage = COMPILE_AREA_STORAGE.init_with(StackStorage::new);
    let mut forth = Forth::new(
        Stack::new_with(data_stack_storage),
        Stack::new_with(return_stack_storage),
        Stack::new_with(compile_area_storage),
    );

    let teletype = Teletype::new(keyboard, display);
    let mut initial_io = CombinedIo::new(StringReader::new(cmforth::FORTH_SOURCE), teletype);
    while !initial_io.reader.is_eof() {
        unsafe { forth.interpret_one(&mut initial_io) }.unwrap();
    }

    info!("Forth interpreter initialized");
    let mut teletype = initial_io.writer;
    loop {
        unsafe {
            let _ = forth.run(&mut teletype).inspect_err(|err| {
                let mut string: heapless::String<256> = heapless::String::new();
                let _ = core::write!(string, "{err}");
                teletype.write(string.as_bytes());
                defmt::error!("Error running forth interpreter: {}", string.as_str());
            });
        }
    }
}

/// Program metadata for `picotool info`
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [rp235x_hal::binary_info::EntryAddr; 5] = [
    rp235x_hal::binary_info::rp_cargo_bin_name!(),
    rp235x_hal::binary_info::rp_cargo_version!(),
    rp235x_hal::binary_info::rp_program_description!(c"cmforth interpreter"),
    rp235x_hal::binary_info::rp_cargo_homepage_url!(),
    rp235x_hal::binary_info::rp_program_build_attribute!(),
];

// End of file

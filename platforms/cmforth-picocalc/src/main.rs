//! PicoCalc display and keyboard bring-up.
#![no_std]
#![no_main]

mod display;
mod keyboard;

use defmt::*;
use defmt_rtt as _;
use display::PicoCalcDisplay;
use embedded_graphics::{
    mono_font::{MonoTextStyleBuilder, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use embedded_hal::i2c::I2c;
use embedded_hal::spi::MODE_0;
use keyboard::PicoCalcKeyboard;
use panic_probe as _;
use rp235x_hal::clocks::init_clocks_and_plls;
use rp235x_hal::fugit::RateExtU32;
use rp235x_hal::gpio::{FunctionSpi, PinState};
use rp235x_hal::{self as hal, entry};
use rp235x_hal::{Clock, pac};

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
    let mut keyboard = PicoCalcKeyboard::new(system_i2c);

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
        12_000_000u32.Hz(),
        MODE_0,
    );

    let mut display = PicoCalcDisplay::new(spi, cs, dc, reset, delay);
    display.init().unwrap();
    display.clear(Rgb888::WHITE).unwrap();

    Rectangle::new(Point::new(8, 8), Size::new(304, 304))
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::WHITE, 3))
        .draw(&mut display)
        .unwrap();

    for (x, color) in [Rgb888::RED, Rgb888::GREEN, Rgb888::BLUE]
        .into_iter()
        .enumerate()
    {
        Rectangle::new(Point::new(45 + x as i32 * 80, 75), Size::new(70, 50))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(&mut display)
            .unwrap();
    }

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb888::WHITE)
        .background_color(Rgb888::BLACK)
        .build();
    Text::with_baseline(
        "Hello, PicoCalc!",
        Point::new(80, 165),
        text_style,
        Baseline::Top,
    )
    .draw(&mut display)
    .unwrap();
    Text::with_baseline(
        "cmforth is running",
        Point::new(70, 195),
        text_style,
        Baseline::Top,
    )
    .draw(&mut display)
    .unwrap();

    const INPUT_LEFT: i32 = 10;
    const INPUT_TOP: i32 = 258;
    const INPUT_BOTTOM: i32 = 308;
    const CHARACTER_WIDTH: i32 = 10;
    const LINE_HEIGHT: i32 = 20;

    Rectangle::new(Point::new(8, 230), Size::new(304, 82))
        .into_styled(PrimitiveStyle::with_fill(Rgb888::BLACK))
        .draw(&mut display)
        .unwrap();
    Text::with_baseline(
        "Keyboard input:",
        Point::new(INPUT_LEFT, 234),
        text_style,
        Baseline::Top,
    )
    .draw(&mut display)
    .unwrap();

    info!("Display and keyboard initialized");

    let mut cursor = Point::new(INPUT_LEFT, INPUT_TOP);
    let mut keyboard_error = false;

    loop {
        let key = match keyboard.read_key() {
            Ok(key) => {
                if keyboard_error {
                    info!("Keyboard communication restored");
                    keyboard_error = false;
                }
                key
            }
            Err(_) => {
                if !keyboard_error {
                    warn!("Could not read keyboard");
                    keyboard_error = true;
                }
                None
            }
        };

        let Some(key) = key else {
            continue;
        };
        debug!("Key pressed: {=u8}", key);

        match key {
            b'\r' | b'\n' => {
                cursor.x = INPUT_LEFT;
                cursor.y += LINE_HEIGHT;
            }
            b'\x08' | b'\x7f' => {
                if cursor.x > INPUT_LEFT {
                    cursor.x -= CHARACTER_WIDTH;
                    Rectangle::new(
                        cursor,
                        Size::new(CHARACTER_WIDTH as u32, LINE_HEIGHT as u32),
                    )
                    .into_styled(PrimitiveStyle::with_fill(Rgb888::BLACK))
                    .draw(&mut display)
                    .unwrap();
                }
            }
            b' '..=b'~' => {
                if cursor.x + CHARACTER_WIDTH > 310 {
                    cursor.x = INPUT_LEFT;
                    cursor.y += LINE_HEIGHT;
                }

                if cursor.y + LINE_HEIGHT > INPUT_BOTTOM {
                    Rectangle::new(
                        Point::new(INPUT_LEFT, INPUT_TOP),
                        Size::new(300, (INPUT_BOTTOM - INPUT_TOP) as u32),
                    )
                    .into_styled(PrimitiveStyle::with_fill(Rgb888::BLACK))
                    .draw(&mut display)
                    .unwrap();
                    cursor = Point::new(INPUT_LEFT, INPUT_TOP);
                }

                let bytes = [key];
                let character = core::str::from_utf8(&bytes).unwrap();
                Text::with_baseline(character, cursor, text_style, Baseline::Top)
                    .draw(&mut display)
                    .unwrap();
                cursor.x += CHARACTER_WIDTH;
            }
            _ => {}
        }

        if cursor.y + LINE_HEIGHT > INPUT_BOTTOM {
            Rectangle::new(
                Point::new(INPUT_LEFT, INPUT_TOP),
                Size::new(300, (INPUT_BOTTOM - INPUT_TOP) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb888::BLACK))
            .draw(&mut display)
            .unwrap();
            cursor = Point::new(INPUT_LEFT, INPUT_TOP);
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

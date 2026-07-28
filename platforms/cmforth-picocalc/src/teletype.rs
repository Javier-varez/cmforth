use core::convert::Infallible;
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::*,
    text::{Baseline, Text},
};
use embedded_hal::{delay::DelayNs, digital::OutputPin, i2c::I2c, spi::SpiBus};
use heapless::string::String;
use heapless::{Deque, Vec};

use crate::display::{HEIGHT, PicoCalcDisplay, WIDTH};
use crate::keyboard::PicoCalcKeyboard;
use cmforth::io::{Reader, ReaderWriter, Writer};

const MAX_LINES: usize = HEIGHT as usize / FONT_10X20.character_size.height as usize;
const MAX_WIDTH: usize =
    WIDTH as usize / (FONT_10X20.character_size.width + FONT_10X20.character_spacing) as usize;

pub struct Teletype<I2C, SPI, CS, DC, RST, DELAY> {
    read_line: Vec<u8, MAX_WIDTH>,
    read_index: usize,
    lines: Deque<String<MAX_WIDTH>, MAX_LINES>,
    keyboard: PicoCalcKeyboard<I2C>,
    display: PicoCalcDisplay<SPI, CS, DC, RST, DELAY>,
}

impl<I2C, SPI, CS, DC, RST, DELAY> Teletype<I2C, SPI, CS, DC, RST, DELAY> {
    pub fn new(
        keyboard: PicoCalcKeyboard<I2C>,
        display: PicoCalcDisplay<SPI, CS, DC, RST, DELAY>,
    ) -> Self {
        Self {
            read_line: Vec::new(),
            read_index: 0,
            lines: Deque::new(),
            keyboard,
            display,
        }
    }
}

impl<I2C, SPI, CS, DC, RST, DELAY> Teletype<I2C, SPI, CS, DC, RST, DELAY>
where
    I2C: I2c,
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    fn read_key(&mut self) -> u8 {
        loop {
            if let Ok(Some(key)) = self.keyboard.read_key() {
                return key;
            }
        }
    }

    fn newline(&mut self) {
        if self.lines.is_full() {
            self.lines.pop_front();
        }
        self.lines.push_back(String::new()).unwrap();
    }

    fn update_display(&mut self) {
        let text_style = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);

        self.display.clear(Rgb888::BLACK).unwrap();
        for (line_number, line) in self.lines.iter().enumerate() {
            Text::with_baseline(
                line,
                Point::new(
                    0,
                    line_number as i32 * FONT_10X20.character_size.height as i32,
                ),
                text_style,
                Baseline::Top,
            )
            .draw(&mut self.display)
            .unwrap();
        }
    }

    fn buffer_line(&mut self) {
        if self.lines.back().is_none_or(|line| !line.is_empty()) {
            self.newline();
        }
        let last_line = self.lines.back_mut().unwrap();
        last_line.push_str("ok ").unwrap();

        self.read_line.clear();
        self.read_index = 0;
        self.update_display();

        let mut exit = false;
        while !exit {
            let v = self.read_key();
            match v {
                b'\x08' | b'\x7f' => {
                    if self
                        .read_line
                        .pop()
                        .is_some_and(|v| v.is_ascii_graphic() || v == b' ')
                    {
                        let last_line = self.lines.back_mut().unwrap();
                        last_line.pop();
                    }
                }
                b'\r' | b'\n' => {
                    self.newline();
                    exit = true;
                }
                v => {
                    let last_line = self.lines.back_mut().unwrap();
                    let printable = v.is_ascii_graphic() || v == b' ';
                    if !self.read_line.is_full()
                        && (!printable || last_line.len() < last_line.capacity())
                    {
                        self.read_line.push(v).unwrap();
                        if printable {
                            last_line.push(v as char).unwrap();
                        }
                    }
                }
            }
            self.update_display();
        }
    }
}

impl<I2C, SPI, CS, DC, RST, DELAY> Reader for Teletype<I2C, SPI, CS, DC, RST, DELAY>
where
    I2C: I2c,
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    fn read(&mut self) -> u8 {
        while self.read_index >= self.read_line.len() {
            self.buffer_line();
        }

        let value = self.read_line[self.read_index];
        self.read_index += 1;
        value
    }

    fn read_word(&mut self) -> &[u8] {
        loop {
            while self
                .read_line
                .get(self.read_index)
                .is_some_and(|v| v.is_ascii_whitespace())
            {
                self.read_index += 1;
            }

            if self.read_index < self.read_line.len() {
                break;
            }

            self.buffer_line();
        }

        let start = self.read_index;
        while self
            .read_line
            .get(self.read_index)
            .is_some_and(|v| !v.is_ascii_whitespace())
        {
            self.read_index += 1;
        }
        &self.read_line[start..self.read_index]
    }
}

impl<I2C, SPI, CS, DC, RST, DELAY> Writer for Teletype<I2C, SPI, CS, DC, RST, DELAY>
where
    I2C: I2c,
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    fn write(&mut self, data: &[u8]) {
        for &v in data {
            match v {
                b'\n' => self.newline(),
                b'\r' => {}
                b' '..=b'~' => {
                    if self
                        .lines
                        .back()
                        .is_none_or(|line| line.len() == line.capacity())
                    {
                        self.newline();
                    }
                    self.lines.back_mut().unwrap().push(v as char).unwrap();
                }
                _ => {}
            }
        }
        self.update_display();
    }
}

impl<I2C, SPI, CS, DC, RST, DELAY> ReaderWriter for Teletype<I2C, SPI, CS, DC, RST, DELAY>
where
    I2C: I2c,
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
}

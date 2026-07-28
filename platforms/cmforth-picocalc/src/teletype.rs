use core::convert::Infallible;
use embedded_graphics::{
    mono_font::{MonoTextStyleBuilder, ascii::FONT_6X12},
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

const CHARACTER_WIDTH: u32 = FONT_6X12.character_size.width + FONT_6X12.character_spacing;
const CHARACTER_HEIGHT: u32 = FONT_6X12.character_size.height;
const MAX_LINES: usize = HEIGHT as usize / CHARACTER_HEIGHT as usize;
const MAX_WIDTH: usize = WIDTH as usize / CHARACTER_WIDTH as usize;

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

    /// Adds a line and reports whether the visible contents scrolled.
    fn newline(&mut self) -> bool {
        let scrolled = self.lines.is_full();
        if scrolled {
            self.lines.pop_front();
        }
        self.lines.push_back(String::new()).unwrap();
        scrolled
    }

    fn update_display(&mut self) {
        let text_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X12)
            .text_color(Rgb888::WHITE)
            .background_color(Rgb888::BLACK)
            .build();

        self.display.clear(Rgb888::BLACK).unwrap();
        for (line_number, line) in self.lines.iter().enumerate() {
            Text::with_baseline(
                line,
                Point::new(0, line_number as i32 * CHARACTER_HEIGHT as i32),
                text_style,
                Baseline::Top,
            )
            .draw(&mut self.display)
            .unwrap();
        }
    }

    fn update_text(&mut self, line: usize, column: usize, text: &str) {
        let text_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X12)
            .text_color(Rgb888::WHITE)
            .background_color(Rgb888::BLACK)
            .build();

        Text::with_baseline(
            text,
            Point::new(
                column as i32 * CHARACTER_WIDTH as i32,
                line as i32 * CHARACTER_HEIGHT as i32,
            ),
            text_style,
            Baseline::Top,
        )
        .draw(&mut self.display)
        .unwrap();
    }

    fn update_character(&mut self, line: usize, column: usize, character: u8) {
        let bytes = [character];
        let text = core::str::from_utf8(&bytes).unwrap();
        self.update_text(line, column, text);
    }

    fn buffer_line(&mut self) {
        if self.lines.back().is_none_or(|line| !line.is_empty()) && self.newline() {
            self.update_display();
        }
        let last_line = self.lines.back_mut().unwrap();
        last_line.push_str("ok ").unwrap();
        let line_number = self.lines.len() - 1;
        self.update_text(line_number, 0, "ok ");

        self.read_line.clear();
        self.read_index = 0;

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
                        let line_number = self.lines.len() - 1;
                        let column = self.lines.back().unwrap().len();
                        self.update_character(line_number, column, b' ');
                    }
                }
                b'\r' | b'\n' => {
                    if self.newline() {
                        self.update_display();
                    }
                    exit = true;
                }
                v => {
                    let printable = v.is_ascii_graphic() || v == b' ';
                    if !self.read_line.is_full()
                        && (!printable
                            || self.lines.back().unwrap().len()
                                < self.lines.back().unwrap().capacity())
                    {
                        self.read_line.push(v).unwrap();
                        if printable {
                            let line_number = self.lines.len() - 1;
                            let column = self.lines.back().unwrap().len();
                            self.lines.back_mut().unwrap().push(v as char).unwrap();
                            self.update_character(line_number, column, v);
                        }
                    }
                }
            }
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
                b'\n' => {
                    if self.newline() {
                        self.update_display();
                    }
                }
                b'\r' => {}
                b' '..=b'~' => {
                    if self
                        .lines
                        .back()
                        .is_none_or(|line| line.len() == line.capacity())
                        && self.newline()
                    {
                        self.update_display();
                    }
                    let line_number = self.lines.len() - 1;
                    let column = self.lines.back().unwrap().len();
                    self.lines.back_mut().unwrap().push(v as char).unwrap();
                    self.update_character(line_number, column, v);
                }
                _ => {}
            }
        }
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

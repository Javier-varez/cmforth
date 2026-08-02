use embedded_graphics::{
    mono_font::{MonoTextStyleBuilder, ascii::FONT_6X12},
    pixelcolor::Rgb888,
    prelude::*,
    text::{Baseline, Text},
};
use embedded_hal::i2c::I2c;
use heapless::string::String;
use heapless::{Deque, Vec};

use crate::display::{DisplayControls, HEIGHT, PicoCalcDisplay, WIDTH};
use crate::keyboard::PicoCalcKeyboard;
use cmforth::io::{Reader, ReaderWriter, Writer};

const CHARACTER_WIDTH: u32 = FONT_6X12.character_size.width + FONT_6X12.character_spacing;
const CHARACTER_HEIGHT: u32 = FONT_6X12.character_size.height;
const MAX_LINES: usize = HEIGHT as usize / CHARACTER_HEIGHT as usize;
const MAX_WIDTH: usize = WIDTH as usize / CHARACTER_WIDTH as usize;

const OK_TEXT: &str = " ok ";

pub struct Teletype<I2C, C: DisplayControls> {
    read_line: Vec<u8, MAX_WIDTH>,
    read_index: usize,
    lines: Deque<String<MAX_WIDTH>, MAX_LINES>,
    keyboard: PicoCalcKeyboard<I2C>,
    display: PicoCalcDisplay<C>,
}

impl<I2C, C: DisplayControls> Teletype<I2C, C> {
    pub fn new(keyboard: PicoCalcKeyboard<I2C>, display: PicoCalcDisplay<C>) -> Self {
        Self {
            read_line: Vec::new(),
            read_index: 0,
            lines: Deque::new(),
            keyboard,
            display,
        }
    }
}

impl<I2C, C> Teletype<I2C, C>
where
    I2C: I2c,
    C: DisplayControls,
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
            self.display
                .scroll_up(CHARACTER_HEIGHT as u16, Rgb888::BLACK)
                .unwrap();
        }
        self.lines.push_back(String::new()).unwrap();
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
        if self.lines.back().is_none_or(|line| !line.is_empty()) {
            self.newline();
        }
        let last_line = self.lines.back_mut().unwrap();
        last_line.push_str(OK_TEXT).unwrap();
        let line_number = self.lines.len() - 1;
        self.update_text(line_number, 0, OK_TEXT);

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
                    self.newline();
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

impl<I2C, C> Reader for Teletype<I2C, C>
where
    I2C: I2c,
    C: DisplayControls,
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
        let word = &self.read_line[start..self.read_index];
        self.read_index += 1; // Skip next whitespace already (if any)
        word
    }

    fn flush(&mut self) {
        self.read_index = self.read_line.len();
    }
}

impl<I2C, C> Writer for Teletype<I2C, C>
where
    I2C: I2c,
    C: DisplayControls,
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

impl<I2C, C> ReaderWriter for Teletype<I2C, C>
where
    I2C: I2c,
    C: DisplayControls,
{
}

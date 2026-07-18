use heapless::Vec;

use crate::io::{IoWriter, Writer};

use core::fmt::Write;

#[derive(Default)]
pub struct SemihostingIo<const MAX_LINE_SIZE: usize = 256> {
    line_buf: Vec<core::ffi::c_char, MAX_LINE_SIZE>,
    idx: usize,
}

impl SemihostingIo {
    pub fn new() -> Self {
        Self::default()
    }

    fn buffer_line(&mut self) {
        self.idx = 0;
        self.line_buf.clear();

        self.write(b" ok ");
        loop {
            const SYS_READC: u32 = 0x07;
            let v = unsafe { cortex_m::asm::semihosting_syscall(SYS_READC, 0) } as u8;

            match v {
                // Delete => backspace
                b'\x7f' => {
                    if self.line_buf.pop().is_some() {
                        self.write(b"\x08 \x08");
                    }
                }
                b'\r' | b'\n' => {
                    self.write(b"\n");
                    break;
                }
                v => {
                    if self.line_buf.push(v).is_err() {
                        let mut writer = IoWriter::new(self);
                        writeln!(writer, " Exceeded maximum line length. Clearing buffer").unwrap();
                        self.line_buf.clear();
                    } else {
                        self.write(&[v]);
                    }
                }
            }
        }
    }

    fn inner_read(&mut self) -> u8 {
        while self.idx >= self.line_buf.len() {
            self.buffer_line();
        }

        let v = self.line_buf[self.idx];
        self.idx += 1;
        v
    }

    fn inner_write(&mut self, v: u8) {
        const SYS_WRITEC: u32 = 0x03;
        unsafe { cortex_m::asm::semihosting_syscall(SYS_WRITEC, (&v as *const _) as u32) };
    }
}

impl super::Reader for SemihostingIo {
    fn read(&mut self) -> u8 {
        self.inner_read()
    }

    fn read_word(&mut self) -> &[u8] {
        loop {
            if self.idx >= self.line_buf.len() {
                self.buffer_line();
            }

            while self.idx < self.line_buf.len() && self.line_buf[self.idx].is_ascii_whitespace() {
                self.idx += 1;
            }

            if self.idx < self.line_buf.len() {
                break;
            }
        }

        let len = self
            .line_buf
            .iter()
            .skip(self.idx)
            .enumerate()
            .find_map(|(i, v)| v.is_ascii_whitespace().then_some(i))
            .unwrap_or(self.line_buf.len() - self.idx);

        let r = &self.line_buf[self.idx..self.idx + len];
        self.idx += len;
        r
    }
}

impl super::Writer for SemihostingIo {
    fn write(&mut self, data: &[u8]) {
        for c in data {
            self.inner_write(*c);
        }
    }
}

impl super::ReaderWriter for SemihostingIo {}

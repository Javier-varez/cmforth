pub trait Reader {
    fn read(&mut self) -> u8;

    fn read_word(&mut self) -> &[u8];
}

pub trait Writer {
    fn write(&mut self, data: &[u8]);
}

pub trait ReaderWriter: Reader + Writer {}

pub struct StringReader<'a> {
    string: &'a [u8],
    idx: usize,
}

impl<'a> StringReader<'a> {
    pub fn new(str: &'a str) -> Self {
        Self {
            string: str.as_bytes(),
            idx: 0,
        }
    }

    pub fn is_eof(&self) -> bool {
        self.idx >= self.string.len()
    }
}

impl Reader for StringReader<'_> {
    fn read(&mut self) -> u8 {
        let v = self.string.get(self.idx).cloned().unwrap_or_else(|| {
            self.idx = self.string.len();
            b' '
        });
        self.idx += 1;
        v
    }

    fn read_word(&mut self) -> &[u8] {
        loop {
            while self
                .string
                .get(self.idx)
                .is_some_and(|v| v.is_ascii_whitespace())
            {
                self.idx += 1;
            }

            if self.string.get(self.idx).is_none_or(|v| *v != b'\\') {
                break;
            }

            while self.string.get(self.idx).is_some_and(|v| *v != b'\n') {
                self.idx += 1;
            }
        }

        let len = self.string[self.idx..]
            .iter()
            .enumerate()
            .find_map(|(i, v)| v.is_ascii_whitespace().then_some(i))
            .unwrap_or(self.string[self.idx..].len());

        let r = &self.string[self.idx..self.idx + len];
        self.idx += len;
        r
    }
}

pub struct CombinedIo<T, U>
where
    T: Reader,
    U: Writer,
{
    pub reader: T,
    pub writer: U,
}

impl<R, W> CombinedIo<R, W>
where
    R: Reader,
    W: Writer,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R, W> Reader for CombinedIo<R, W>
where
    R: Reader,
    W: Writer,
{
    fn read_word(&mut self) -> &[u8] {
        self.reader.read_word()
    }

    fn read(&mut self) -> u8 {
        self.reader.read()
    }
}

impl<R, W> Writer for CombinedIo<R, W>
where
    R: Reader,
    W: Writer,
{
    fn write(&mut self, data: &[u8]) {
        self.writer.write(data);
    }
}

impl<R, W> ReaderWriter for CombinedIo<R, W>
where
    R: Reader,
    W: Writer,
{
}

pub struct IoWriter<'a, T>
where
    T: Writer,
{
    io: &'a mut T,
}

impl<'a, T: Writer> IoWriter<'a, T> {
    pub fn new(io: &'a mut T) -> Self {
        Self { io }
    }
}

impl<'a, T> core::fmt::Write for IoWriter<'a, T>
where
    T: Writer,
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.io.write(s.as_bytes());
        Ok(())
    }
}

#[cfg(all(target_os = "none", target_arch = "arm", feature = "cortex-m-arch"))]
mod thumbv7m;

#[cfg(all(target_os = "none", target_arch = "arm", feature = "cortex-m-arch"))]
pub use thumbv7m::*;

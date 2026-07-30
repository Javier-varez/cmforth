//! Hardware-backed implementation of the display-control traits.
//!
//! This module provides [`HardwareControls`], a concrete implementation of
//! [`DisplayControls`] that talks to the ST7365P display controller over an
//! embedded-hal SPI bus plus chip-select (CS), data/command (D/C), and reset
//! GPIO pins. It handles the low-level transaction details: asserting CS for
//! the duration of each transfer, toggling D/C to distinguish command bytes
//! from parameter and pixel data, and honoring the controller's minimum
//! 40 ns CS-high time between transactions.
//!
//! Pixel writes are performed through [`HardwareWriteOp`], which programs
//! the controller's column/page address window, issues a memory-write
//! command, and then streams pixel data while CS stays asserted. Dropping
//! the operation without finishing it still flushes and deselects the
//! display, so an abandoned write never leaves the bus selected.

use core::convert::Infallible;

use embedded_graphics::{pixelcolor::Rgb888, prelude::*};
use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiBus};

use super::traits::{BatchedPixelWriteOp, DisplayControls};

const COLUMN_ADDRESS_SET: u8 = 0x2a;
const PAGE_ADDRESS_SET: u8 = 0x2b;
const MEMORY_WRITE: u8 = 0x2c;

// The ST7365P requires CS to remain high for at least 40 ns between
// transactions.
const CS_HIGH_DELAY_NS: u32 = 40;

/// Hardware-backed implementation of [`DisplayControls`].
pub struct HardwareControls<SPI, CS, DC, RST, DELAY> {
    spi: SPI,
    cs: CS,
    dc: DC,
    reset: RST,
    delay: DELAY,
}

impl<SPI, CS, DC, RST, DELAY> HardwareControls<SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    pub fn new(spi: SPI, cs: CS, dc: DC, reset: RST, delay: DELAY) -> Self {
        Self {
            spi,
            cs,
            dc,
            reset,
            delay,
        }
    }

    /// Sends the given bytes as a command or data transaction
    fn send_command_or_data(&mut self, data: bool, bytes: &[u8]) -> Result<(), SPI::Error> {
        if data {
            self.dc.set_high().unwrap();
        } else {
            self.dc.set_low().unwrap();
        }

        self.select();
        let result = self.spi.write(bytes).and_then(|()| self.spi.flush());
        self.deselect();
        result
    }

    /// Engages chip select.
    fn select(&mut self) {
        self.cs.set_low().unwrap();
    }

    /// Disengages chip select.
    fn deselect(&mut self) {
        self.cs.set_high().unwrap();
        self.delay.delay_ns(CS_HIGH_DELAY_NS);
    }
}

/// An active pixel-write operation on the hardware display.
///
/// Buffers pixels internally and flushes them to the SPI bus in
/// scanline-sized chunks to amortize transfer overhead.
pub struct HardwareWriteOp<'a, SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    controls: &'a mut HardwareControls<SPI, CS, DC, RST, DELAY>,
}

impl<'a, SPI, CS, DC, RST, DELAY> HardwareWriteOp<'a, SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    fn initiate(
        controls: &'a mut HardwareControls<SPI, CS, DC, RST, DELAY>,
        x_start: u16,
        y_start: u16,
        x_end: u16,
        y_end: u16,
    ) -> Result<Self, SPI::Error> {
        let columns = [
            (x_start >> 8) as u8,
            x_start as u8,
            (x_end >> 8) as u8,
            x_end as u8,
        ];
        let pages = [
            (y_start >> 8) as u8,
            y_start as u8,
            (y_end >> 8) as u8,
            y_end as u8,
        ];

        controls.select();

        let result = (|| -> Result<(), SPI::Error> {
            controls.dc.set_low().unwrap();
            controls.spi.write(&[COLUMN_ADDRESS_SET])?;
            controls.spi.flush()?;
            controls.dc.set_high().unwrap();
            controls.spi.write(&columns)?;
            controls.spi.flush()?;

            controls.dc.set_low().unwrap();
            controls.spi.write(&[PAGE_ADDRESS_SET])?;
            controls.spi.flush()?;
            controls.dc.set_high().unwrap();
            controls.spi.write(&pages)?;
            controls.spi.flush()?;

            controls.dc.set_low().unwrap();
            controls.spi.write(&[MEMORY_WRITE])?;
            controls.spi.flush()?;
            controls.dc.set_high().unwrap();

            Ok(())
        })();

        result.inspect_err(|_| {
            controls.deselect();
        })?;

        Ok(Self { controls })
    }

    fn finish_impl(&mut self) -> Result<(), SPI::Error> {
        let result = self.controls.spi.flush();
        self.controls.deselect();
        result
    }
}

impl<SPI, CS, DC, RST, DELAY> BatchedPixelWriteOp for HardwareWriteOp<'_, SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    type Error = SPI::Error;

    fn push_pixel(&mut self, color: Rgb888) -> Result<(), Self::Error> {
        let pix_buffer = &[color.r(), color.g(), color.b()];
        let result = self.controls.spi.write(pix_buffer);
        if result.is_err() {
            self.controls.deselect();
        }
        Ok(())
    }

    fn finish(mut self) -> Result<(), Self::Error> {
        let result = self.finish_impl();
        let _ = core::mem::ManuallyDrop::new(self);
        result
    }
}

impl<SPI, CS, DC, RST, DELAY> Drop for HardwareWriteOp<'_, SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    fn drop(&mut self) {
        let _ = self.finish_impl();
    }
}

impl<SPI, CS, DC, RST, DELAY> DisplayControls for HardwareControls<SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    type Error = SPI::Error;

    type WriteOp<'a>
        = HardwareWriteOp<'a, SPI, CS, DC, RST, DELAY>
    where
        Self: 'a;

    fn send_command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.send_command_or_data(false, &[command])
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        if data.is_empty() {
            return Ok(());
        }
        self.send_command_or_data(true, data)
    }

    fn begin_pixel_write(
        &mut self,
        x_start: u16,
        y_start: u16,
        x_end: u16,
        y_end: u16,
    ) -> Result<Self::WriteOp<'_>, Self::Error> {
        HardwareWriteOp::initiate(self, x_start, y_start, x_end, y_end)
    }

    fn hardware_reset(&mut self) {
        self.cs.set_high().unwrap();
        self.dc.set_high().unwrap();

        self.reset.set_high().unwrap();
        self.delay.delay_ms(10);
        self.reset.set_low().unwrap();
        self.delay.delay_ms(10);
        self.reset.set_high().unwrap();
        self.delay.delay_ms(200);
    }

    fn delay_ms(&mut self, ms: u32) {
        self.delay.delay_ms(ms);
    }
}

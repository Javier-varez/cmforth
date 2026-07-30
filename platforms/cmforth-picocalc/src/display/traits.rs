//! Abstractions over the display controller's low-level hardware interface.
//!
//! This module defines the two traits that decouple [`PicoCalcDisplay`](super::PicoCalcDisplay)'s
//! display logic from the SPI bus, GPIO pins, and delay timer it runs on:
//!
//! - [`DisplayControls`] is the main interface: single command and data
//!   transfers, hardware reset, delays, and starting pixel-write operations.
//! - [`BatchedPixelWriteOp`] is the handle returned by
//!   [`DisplayControls::begin_pixel_write`], streaming pixels into an
//!   address window in batches until the write is finished.
//!
//! Splitting the hardware access behind these traits keeps
//! [`PicoCalcDisplay`](super::PicoCalcDisplay) non-generic over the concrete bus and pin types and
//! allows the display logic to be tested against mock implementations. See
//! the `control` submodule for the hardware-backed implementation.

use embedded_graphics::pixelcolor::Rgb888;

/// An incremental pixel-write operation on the display.
///
/// Returned by [`DisplayControls::begin_pixel_write`]. Buffers pixel data
/// internally, flushing it to the display in batches, until the operation
/// is finalized.
pub trait BatchedPixelWriteOp {
    type Error;

    /// Push one 18-bit RGB pixel into the operation, flushing the internal
    /// buffer to the display when it fills up.
    fn push_pixel(&mut self, pixel: Rgb888) -> Result<(), Self::Error>;

    /// Flush any buffered pixels and finalize the write operation,
    /// deselecting the display.
    fn finish(self) -> Result<(), Self::Error>;
}

/// Low-level display hardware controls.
///
/// This trait abstracts the SPI bus, GPIO pins, and delay timer needed to
/// communicate with the display controller. Implementors handle the
/// hardware-specific details so that [`PicoCalcDisplay`](super::PicoCalcDisplay) can focus on
/// display logic without being generic over the hardware types.
pub trait DisplayControls {
    type Error: core::fmt::Debug;
    type WriteOp<'a>: BatchedPixelWriteOp<Error = Self::Error>
    where
        Self: 'a;

    /// Send a single command byte to the display controller.
    fn send_command(&mut self, command: u8) -> Result<(), Self::Error>;

    /// Send parameter or pixel data bytes to the display controller.
    fn send_data(&mut self, data: &[u8]) -> Result<(), Self::Error>;

    /// Send a command byte followed by its parameter data.
    fn send_command_with_data(&mut self, command: u8, data: &[u8]) -> Result<(), Self::Error> {
        self.send_command(command)?;
        if !data.is_empty() {
            self.send_data(data)?;
        }
        Ok(())
    }

    /// Begin an incremental pixel-write operation for the given address
    /// window. Returns a handle that can be used to write pixel data in
    /// batches.
    fn begin_pixel_write(
        &mut self,
        x_start: u16,
        y_start: u16,
        x_end: u16,
        y_end: u16,
    ) -> Result<Self::WriteOp<'_>, Self::Error>;

    /// Perform a hardware reset of the display controller.
    fn hardware_reset(&mut self);

    /// Busy-wait for the given number of milliseconds.
    fn delay_ms(&mut self, ms: u32);
}

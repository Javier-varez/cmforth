//! Driver for the PicoCalc's 4-inch, 320 × 320 IPS TFT display.
//!
//! The panel uses an ST7365P controller with an ILI9488-compatible command
//! set. ClockworkPi's reference firmware identifies it as an ILI9488. This
//! module implements a write-only [`DrawTarget`] using SPI mode 0 and the
//! reference initialization sequence. The controller is configured for
//! 18-bit RGB666 color, represented by three bytes per [`Rgb888`] pixel.
//!
//! # Pinout
//!
//! The display is connected to the RP2350's SPI1 peripheral:
//!
//! | Pico GPIO | Display signal | Expected function |
//! |-----------|----------------|-------------------|
//! | GP10 | SCK | SPI1 serial-clock output. |
//! | GP11 | MOSI/SDI | SPI1 data output carrying commands, parameters, and pixels. |
//! | GP12 | MISO/SDO | SPI1 data input for controller and display-memory reads; configured by the board setup but unused by this write-only driver. |
//! | GP13 | CS | Active-low chip select. It must remain high for at least 40 ns between transactions. |
//! | GP14 | D/C | Command/data selection: low for commands, high for parameters and pixel data. |
//! | GP15 | RESET | Active-low hardware reset for the display controller. |
//!
//! The LCD backlight is not connected to an RP2350 GPIO. It is controlled by
//! the PicoCalc's STM32 system controller over I2C1 on GP6 (SDA) and GP7 (SCL)
//! and must be enabled separately from this display driver.

use core::convert::Infallible;

mod control;
mod traits;

use control::HardwareControls;
use embedded_graphics::{pixelcolor::Rgb888, prelude::*, primitives::Rectangle};
use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiBus};
pub use traits::{BatchedPixelWriteOp, DisplayControls};

pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 320;

const GRAM_HEIGHT: u16 = 480;
const VERTICAL_SCROLLING_DEFINITION: u8 = 0x33;
const VERTICAL_SCROLLING_START_ADDRESS: u8 = 0x37;

/// Write-only driver for the PicoCalc display.
pub struct PicoCalcDisplay<C: DisplayControls> {
    controls: C,
    scroll_offset: u16,
}

impl<SPI, CS, DC, RST, DELAY> PicoCalcDisplay<HardwareControls<SPI, CS, DC, RST, DELAY>>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    pub fn new(spi: SPI, cs: CS, dc: DC, reset: RST, delay: DELAY) -> Self {
        Self {
            controls: HardwareControls::new(spi, cs, dc, reset, delay),
            scroll_offset: 0,
        }
    }
}

impl<C: DisplayControls> PicoCalcDisplay<C> {
    /// Reset and initialize the panel using ClockworkPi's reference sequence.
    pub fn init(&mut self) -> Result<(), C::Error> {
        self.controls.hardware_reset();

        self.controls.send_command_with_data(
            0xe0,
            &[
                0x00, 0x03, 0x09, 0x08, 0x16, 0x0a, 0x3f, 0x78, 0x4c, 0x09, 0x0a, 0x08, 0x16, 0x1a,
                0x0f,
            ],
        )?;
        self.controls.send_command_with_data(
            0xe1,
            &[
                0x00, 0x16, 0x19, 0x03, 0x0f, 0x05, 0x32, 0x45, 0x46, 0x04, 0x0e, 0x0d, 0x35, 0x37,
                0x0f,
            ],
        )?;
        self.controls.send_command_with_data(0xc0, &[0x17, 0x15])?;
        self.controls.send_command_with_data(0xc1, &[0x41])?;
        self.controls
            .send_command_with_data(0xc5, &[0x00, 0x12, 0x80])?;
        self.controls.send_command_with_data(0x36, &[0x48])?; // MX and BGR order
        self.controls.send_command_with_data(0x3a, &[0x66])?; // 18-bit RGB over SPI
        self.controls.send_command_with_data(0xb0, &[0x00])?;
        self.controls.send_command_with_data(0xb1, &[0xa0])?;
        self.controls.send_command_with_data(0x21, &[])?; // Display inversion on
        self.controls.send_command_with_data(0xb4, &[0x02])?;
        self.controls
            .send_command_with_data(0xb6, &[0x02, 0x02, 0x3b])?;
        self.controls.send_command_with_data(0xb7, &[0xc6])?;
        self.controls.send_command_with_data(0xe9, &[0x00])?;
        self.controls
            .send_command_with_data(0xf7, &[0xa9, 0x51, 0x2c, 0x82])?;

        self.controls.send_command_with_data(0x11, &[])?; // Sleep out
        self.controls.delay_ms(120);
        self.controls.send_command_with_data(0x29, &[])?; // Display on
        self.controls.delay_ms(120);
        self.controls.send_command_with_data(0x36, &[0x48])?;

        // The controller has 480 rows of GRAM behind the 320-row panel. Make
        // the entire GRAM area scrollable so it can be used as a circular
        // backing store for the terminal viewport.
        let [scroll_height_high, scroll_height_low] = GRAM_HEIGHT.to_be_bytes();
        self.controls.send_command_with_data(
            VERTICAL_SCROLLING_DEFINITION,
            &[
                0x00,
                0x00,
                scroll_height_high,
                scroll_height_low,
                0x00,
                0x00,
            ],
        )?;
        self.write_scroll_offset(0)?;
        self.scroll_offset = 0;

        Ok(())
    }

    /// Scrolls the visible viewport up and clears the newly exposed rows.
    pub(crate) fn scroll_up(&mut self, rows: u16, color: Rgb888) -> Result<(), C::Error> {
        if rows == 0 {
            return Ok(());
        }

        let old_offset = self.scroll_offset;
        let offset = (self.scroll_offset + rows) % GRAM_HEIGHT;
        // Use the next mapping to clear the rows that are about to enter the
        // viewport while the controller still keeps them off-screen.
        self.scroll_offset = offset;
        let area = Rectangle::new(
            Point::new(0, (HEIGHT - rows) as i32),
            Size::new(WIDTH as u32, rows as u32),
        );
        if let Err(error) = self.fill_solid(&area, color) {
            self.scroll_offset = old_offset;
            return Err(error);
        }

        if let Err(error) = self.write_scroll_offset(offset) {
            self.scroll_offset = old_offset;
            return Err(error);
        }

        Ok(())
    }

    fn write_scroll_offset(&mut self, offset: u16) -> Result<(), C::Error> {
        self.controls
            .send_command_with_data(VERTICAL_SCROLLING_START_ADDRESS, &offset.to_be_bytes())
    }
}

impl<C: DisplayControls> OriginDimensions for PicoCalcDisplay<C> {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl<C: DisplayControls> DrawTarget for PicoCalcDisplay<C> {
    type Color = Rgb888;
    type Error = C::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let scroll_offset = self.scroll_offset;
        let mut iter = pixels.into_iter().peekable();

        while let Some(&Pixel(first_point, _)) = iter.peek() {
            if first_point.x < 0
                || first_point.y < 0
                || first_point.x >= WIDTH as i32
                || first_point.y >= HEIGHT as i32
            {
                iter.next();
                continue;
            }

            let physical_y = (scroll_offset + first_point.y as u16) % GRAM_HEIGHT;
            let window_start_x = first_point.x;

            let mut op = self.controls.begin_pixel_write(
                first_point.x as u16,
                physical_y,
                WIDTH - 1,
                GRAM_HEIGHT - 1,
            )?;

            // Consume pixels while they form a contiguous run.
            let mut expected = Some(first_point);
            while let Some(&Pixel(point, color)) = iter.peek() {
                if expected != Some(point) {
                    break;
                }

                op.push_pixel(color)?;

                let py = (scroll_offset + point.y as u16) % GRAM_HEIGHT;
                expected = if point.x < WIDTH as i32 - 1 {
                    Some(Point::new(point.x + 1, point.y))
                } else if point.y < HEIGHT as i32 - 1 && py < GRAM_HEIGHT - 1 {
                    Some(Point::new(window_start_x, point.y + 1))
                } else {
                    None
                };

                iter.next();
            }

            op.finish()?;
        }

        Ok(())
    }
}

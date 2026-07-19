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

use embedded_graphics::{pixelcolor::Rgb888, prelude::*, primitives::Rectangle};
use embedded_hal::{delay::DelayNs, digital::OutputPin, spi::SpiBus};

pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 320;

const COLUMN_ADDRESS_SET: u8 = 0x2a;
const PAGE_ADDRESS_SET: u8 = 0x2b;
const MEMORY_WRITE: u8 = 0x2c;

// The ST7365P requires CS to remain high for at least 40 ns between
// transactions.
const CS_HIGH_DELAY_NS: u32 = 40;

/// Write-only driver for the PicoCalc's ILI9488 display.
pub struct PicoCalcDisplay<SPI, CS, DC, RST, DELAY> {
    spi: SPI,
    cs: CS,
    dc: DC,
    reset: RST,
    delay: DELAY,
}

impl<SPI, CS, DC, RST, DELAY> PicoCalcDisplay<SPI, CS, DC, RST, DELAY>
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

    /// Reset and initialize the panel using ClockworkPi's reference sequence.
    pub fn init(&mut self) -> Result<(), SPI::Error> {
        self.cs.set_high().unwrap();
        self.dc.set_high().unwrap();

        self.reset.set_high().unwrap();
        self.delay.delay_ms(10);
        self.reset.set_low().unwrap();
        self.delay.delay_ms(10);
        self.reset.set_high().unwrap();
        self.delay.delay_ms(200);

        self.command(
            0xe0,
            &[
                0x00, 0x03, 0x09, 0x08, 0x16, 0x0a, 0x3f, 0x78, 0x4c, 0x09, 0x0a, 0x08, 0x16, 0x1a,
                0x0f,
            ],
        )?;
        self.command(
            0xe1,
            &[
                0x00, 0x16, 0x19, 0x03, 0x0f, 0x05, 0x32, 0x45, 0x46, 0x04, 0x0e, 0x0d, 0x35, 0x37,
                0x0f,
            ],
        )?;
        self.command(0xc0, &[0x17, 0x15])?;
        self.command(0xc1, &[0x41])?;
        self.command(0xc5, &[0x00, 0x12, 0x80])?;
        self.command(0x36, &[0x48])?; // MX and BGR order
        self.command(0x3a, &[0x66])?; // 18-bit RGB over SPI
        self.command(0xb0, &[0x00])?;
        self.command(0xb1, &[0xa0])?;
        self.command(0x21, &[])?; // Display inversion on
        self.command(0xb4, &[0x02])?;
        self.command(0xb6, &[0x02, 0x02, 0x3b])?;
        self.command(0xb7, &[0xc6])?;
        self.command(0xe9, &[0x00])?;
        self.command(0xf7, &[0xa9, 0x51, 0x2c, 0x82])?;

        self.command(0x11, &[])?; // Sleep out
        self.delay.delay_ms(120);
        self.command(0x29, &[])?; // Display on
        self.delay.delay_ms(120);
        self.command(0x36, &[0x48])?;

        Ok(())
    }

    fn write_init(&mut self, data: bool, bytes: &[u8]) -> Result<(), SPI::Error> {
        if data {
            self.dc.set_high().unwrap();
        } else {
            self.dc.set_low().unwrap();
        }

        self.cs.set_low().unwrap();
        let result = self.spi.write(bytes).and_then(|()| self.spi.flush());
        self.deselect();
        result
    }

    fn command(&mut self, command: u8, data: &[u8]) -> Result<(), SPI::Error> {
        // Keep command and data in separate transactions so CS returns high
        // before D/C changes, but send all parameter bytes in one transfer.
        self.write_init(false, &[command])?;
        if !data.is_empty() {
            self.write_init(true, data)?;
        }
        Ok(())
    }

    fn begin_write(
        &mut self,
        x_start: u16,
        y_start: u16,
        x_end: u16,
        y_end: u16,
    ) -> Result<(), SPI::Error> {
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

        self.cs.set_low().unwrap();

        let result = (|| -> Result<(), SPI::Error> {
            self.dc.set_low().unwrap();
            self.spi.write(&[COLUMN_ADDRESS_SET])?;
            self.spi.flush()?;
            self.dc.set_high().unwrap();
            self.spi.write(&columns)?;
            self.spi.flush()?;

            self.dc.set_low().unwrap();
            self.spi.write(&[PAGE_ADDRESS_SET])?;
            self.spi.flush()?;
            self.dc.set_high().unwrap();
            self.spi.write(&pages)?;
            self.spi.flush()?;

            self.dc.set_low().unwrap();
            self.spi.write(&[MEMORY_WRITE])?;
            self.spi.flush()?;
            self.dc.set_high().unwrap();

            Ok(())
        })();

        if result.is_err() {
            self.deselect();
        }

        result
    }

    fn end_write(&mut self) -> Result<(), SPI::Error> {
        let result = self.spi.flush();
        self.deselect();
        result
    }

    fn write_color(&mut self, color: Rgb888) -> Result<(), SPI::Error> {
        self.spi.write(&[color.r(), color.g(), color.b()])
    }

    fn abort_write(&mut self) {
        self.deselect();
    }

    fn deselect(&mut self) {
        self.cs.set_high().unwrap();
        self.delay.delay_ns(CS_HIGH_DELAY_NS);
    }
}

impl<SPI, CS, DC, RST, DELAY> OriginDimensions for PicoCalcDisplay<SPI, CS, DC, RST, DELAY> {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl<SPI, CS, DC, RST, DELAY> DrawTarget for PicoCalcDisplay<SPI, CS, DC, RST, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin<Error = Infallible>,
    DC: OutputPin<Error = Infallible>,
    RST: OutputPin<Error = Infallible>,
    DELAY: DelayNs,
{
    type Color = Rgb888;
    type Error = SPI::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let mut selected = false;
        let mut expected = None;
        let mut window_start_x = 0;

        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 || point.x >= WIDTH as i32 || point.y >= HEIGHT as i32 {
                continue;
            }

            if expected != Some(point) {
                if selected {
                    self.end_write()?;
                }

                window_start_x = point.x;
                self.begin_write(point.x as u16, point.y as u16, WIDTH - 1, HEIGHT - 1)?;
                selected = true;
            }

            if let Err(error) = self.write_color(color) {
                self.abort_write();
                return Err(error);
            }

            expected = if point.x < WIDTH as i32 - 1 {
                Some(Point::new(point.x + 1, point.y))
            } else if point.y < HEIGHT as i32 - 1 {
                Some(Point::new(window_start_x, point.y + 1))
            } else {
                None
            };
        }

        if selected {
            self.end_write()?;
        }

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let drawable = area.intersection(&self.bounding_box());
        let Some(bottom_right) = drawable.bottom_right() else {
            return Ok(());
        };

        self.begin_write(
            drawable.top_left.x as u16,
            drawable.top_left.y as u16,
            bottom_right.x as u16,
            bottom_right.y as u16,
        )?;

        for (point, color) in area.points().zip(colors) {
            if drawable.contains(point)
                && let Err(error) = self.write_color(color)
            {
                self.abort_write();
                return Err(error);
            }
        }

        self.end_write()
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        const CHUNK_PIXELS: usize = 32;

        let drawable = area.intersection(&self.bounding_box());
        let Some(bottom_right) = drawable.bottom_right() else {
            return Ok(());
        };

        self.begin_write(
            drawable.top_left.x as u16,
            drawable.top_left.y as u16,
            bottom_right.x as u16,
            bottom_right.y as u16,
        )?;

        let mut buffer = [0; CHUNK_PIXELS * 3];
        for pixel in buffer.chunks_exact_mut(3) {
            pixel.copy_from_slice(&[color.r(), color.g(), color.b()]);
        }

        let pixel_count = drawable.size.width as usize * drawable.size.height as usize;
        let full_chunks = pixel_count / CHUNK_PIXELS;
        let remaining_bytes = pixel_count % CHUNK_PIXELS * 3;

        for _ in 0..full_chunks {
            if let Err(error) = self.spi.write(&buffer) {
                self.abort_write();
                return Err(error);
            }
        }

        if remaining_bytes != 0
            && let Err(error) = self.spi.write(&buffer[..remaining_bytes])
        {
            self.abort_write();
            return Err(error);
        }

        self.end_write()
    }
}

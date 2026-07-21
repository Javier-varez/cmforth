//! Driver for the PicoCalc keyboard controller.
//!
//! The keyboard is scanned by the PicoCalc's STM32 system controller. Key
//! events are read over I2C1 on GP6 (SDA) and GP7 (SCL), which is the same bus
//! used to control the LCD backlight.

use embedded_hal::i2c::I2c;

const CONTROLLER_ADDRESS: u8 = 0x1f;
const KEY_EVENT_REGISTER: u8 = 0x09;
const CONTROL_KEY: u8 = 0x7e;

const KEY_PRESSED: u8 = 0x01;
const CONTROL_PRESSED: u8 = 0x02;
const CONTROL_RELEASED: u8 = 0x03;

/// Polling driver for the PicoCalc keyboard.
pub struct PicoCalcKeyboard<I2C> {
    i2c: I2C,
    control_held: bool,
}

impl<I2C> PicoCalcKeyboard<I2C>
where
    I2C: I2c,
{
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            control_held: false,
        }
    }

    /// Polls the controller once and returns a newly pressed key, if any.
    ///
    /// The controller returns two bytes: an event type followed by the key's
    /// ASCII value. Control key transitions are consumed by the driver, and
    /// Ctrl+A through Ctrl+Z are returned as ASCII control characters.
    pub fn read_key(&mut self) -> Result<Option<u8>, I2C::Error> {
        let mut event = [0; 2];
        self.i2c
            .write_read(CONTROLLER_ADDRESS, &[KEY_EVENT_REGISTER], &mut event)?;

        match event {
            [CONTROL_PRESSED, CONTROL_KEY] => {
                self.control_held = true;
                Ok(None)
            }
            [CONTROL_RELEASED, CONTROL_KEY] => {
                self.control_held = false;
                Ok(None)
            }
            [KEY_PRESSED, mut key] => {
                if self.control_held && key.is_ascii_lowercase() {
                    key = key - b'a' + 1;
                }
                Ok(Some(key))
            }
            _ => Ok(None),
        }
    }
}

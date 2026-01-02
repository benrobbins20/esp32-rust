use std::time::Duration;

use anyhow::{Result, Ok};
use esp_idf_hal::{gpio::{OutputPin, Pins}, peripheral::Peripheral, prelude::Peripherals, rmt::{FixedLengthSignal, PinState, Pulse, RmtChannel, TxRmtDriver, config::TransmitConfig}};
use rgb::{Rgba};
use rgb::RGB;

// thread safe counter for 0-255 for led color shift
static position: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
// mechanism to convert hex string to RGB color 
#[derive(Debug, Clone, Copy)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    pub fn new(
        r: u8,
        g: u8,
        b: u8
    ) -> Result<Self> {
        Ok(Color { r, g, b })
    }
}
impl TryFrom<&str> for Color {
    type Error = anyhow::Error;
    
    fn try_from(input: &str) -> anyhow::Result<Self> {
        Ok(Color {
            g: u8::from_str_radix(&input[0..2], 16)?,
            r: u8::from_str_radix(&input[2..4], 16)?,
            b: u8::from_str_radix(&input[4..6], 16)?
        })
    }
}

// conversion to rgba struct
impl From<Color> for Rgba<u8> {
    fn from(color: Color) -> Self {
        Rgba::new(color.g, color.r, color.b, 255)
    }
}

// conversion to u32 for bitmask
impl From<Color> for u32 {
    fn from(value: Color) -> Self {
        ((value.g as u32) << 16) | ((value.r as u32) << 8) | (value.b as u32)
    }
}


// transmit driver for WS2812 RMT
pub struct RMTDriver<'a> {
    driver: TxRmtDriver<'a>,
}
impl<'a> RMTDriver<'a> {
    pub fn new(
        // pass in P::RmtChannel and P::OutputPin
        channel: impl Peripheral<P = impl RmtChannel> + 'a,
        pin: impl Peripheral<P = impl OutputPin> + 'a,
    ) -> Result<Self> {
        let config = TransmitConfig::new()
            .clock_divider(2); // 160MHz / 2 = 80MHz ticks
        let driver = TxRmtDriver::new(channel, pin, &config)?;

        
        Ok(RMTDriver {
            driver
        })
    }

    
    fn set_signal(&mut self, h1: u64, l1: u64, h0: u64, l0: u64, color: Color) -> Result<FixedLengthSignal<24>>{
        let mut signal = FixedLengthSignal::<24>::new();
        let ticks_hz = self.driver.counter_clock()?;

        // you send a 24 bit packet to WS2812 with each bit being set in fixed length buffer
        // each bit is sent as a pair of high/low pulses in a pre-defined interval
        /* From the datasheet
        T0H 0 code ,high voltage time 0.4us ±150ns 
        T1H 1 code ,high voltage time 0.8us ±150ns
        T0L 0 code , low voltage time 0.85us ±150ns
        T1L 1 code ,low voltage time 0.45us ±150ns
        */
        // (high:t1h, low:t1l) for 1 bit
        let t1h = Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(h1))?;
        let t1l = Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(l1))?;
        // (high:t0h, low:t0l) for 0 bit
        let t0h = Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(h0))?;
        let t0l = Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(l0))?;
        
        // loop MSB first through 24 bits
        for i in (0..24).rev() {
            let bit_mask = 2u32.pow(i); // bit mask for the current color bit
            
            let bit_bool = (u32::from(color) & bit_mask) != 0; // bit boolean, true if 1, false if 0
            
            // for each bit, set the tuple of (high, low) pulses
            let (high, low) = if bit_bool {
                (t1h, t1l)
            }
            else {
                (t0h, t0l)
            };
            // set the signal per bit, decrementing size
            signal.set(23 - i as usize, &(high, low))?;
        }
        // 
        Ok(signal)
    }

    pub fn set_rgb(&mut self, color: Result<Color>) -> Result<()> {
        let color = color?;
        log::info!("Setting RGB to R:{:02X} G:{:02X} B:{:02X}", color.r, color.g, color.b);
        let signal = Self::set_signal(self, 800, 450, 400, 850, color)?;
        self.driver.start_blocking(&signal)?;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        let black = Color { r: 0, g: 0, b: 0 };
        let signal = Self::set_signal(self, 800, 450, 400, 850, black)?;
        self.driver.start_blocking(&signal)?;
        Ok(())
    }

    pub fn color_shifter() -> Result<Color> {
        // array of bytes to store RGB values, split this up into RGB struct return
        let rgb_val: [u8;3];
        // everytime function is called, atomic increment static pos counter
        // name this outer position to tell which is global and which is local which is zeroed in each segment
        let outer_pos = position.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    
        // initially starting 255, 0, 0 (red)
        if (outer_pos < 85) {
            print!("\rR->B");                // return print cursor to beginning of line for less print noise
            let inner_pos = outer_pos;       // outer already at zero but define a local position counter
            let r = 255 - (inner_pos * 3);   // red decreases to 0; 0:84
            let g = 0;                       // green is off
            let b = inner_pos * 3;           // blue increases to 255
            rgb_val = [r, g, b];
        }
        else if (outer_pos < 170) {
            print!("\rB->G");
            let inner_pos = outer_pos - 85;   // continue from 0 again (85-85)
            let r = 0;                      
            let g = inner_pos * 3;            // green grows from 0:255
            let b = 255 - (inner_pos * 3);    // blue shrinks from 255:0
            rgb_val = [r, g, b];
        }
        else {
            print!("\rG->R");
            let inner_pos = outer_pos - 170;    // zero the counter
            let r = inner_pos * 3;              // red grows
            let g = 255 - (inner_pos * 3);      // green shrinks
            let b = 0;
            rgb_val = [r, g, b];
        }
    
        Color::new(rgb_val[0], rgb_val[1], rgb_val[2])
    }

}




// fn set_rgb(color: u32, driver: &mut TxRmtDriver) -> Result<()> {

// // loop through each bit and send the pulse sequence
// // MSB first
// for i in (0..24).rev() {
    
// driver.start_blocking(&signal)?;

// Ok(())
// }

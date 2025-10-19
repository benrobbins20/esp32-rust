use std::{default, time::Duration};
use anyhow::Result;
use esp_idf_svc::{eventloop::EspSystemEventLoop, hal::prelude::Peripherals};
use esp_idf_hal::{delay::FreeRtos, rmt::{config::TransmitConfig, FixedLengthSignal, PinState, Pulse, PulseTicks, TxRmtDriver}};

// mod rgb;


// // bring in secrets
#[toml_cfg::toml_config]
pub struct WifiConfig {
    #[default("wifi_ssid")]
    wifi_ssid: &'static str,
    #[default("wifi_password")]
    wifi_password: &'static str
}

// fn u32_to_GRB(color: u32) -> (u8, u8, u8) {
//     let green = (color & 0xFF0000);
//     let red = (color & 0x00FF00);
//     let blue = (color & 0x0000FF);
// }

fn send_frame(color: u32, driver: &mut TxRmtDriver) -> Result<()> {
    
        // WS2812 is GRB
        // let mut blue = 0u32;
        // let mut green = 0u32;
        // let mut red = 0u32;
        // let color:u32 = (green << 16) | (red << 8) | blue;
        log::info!("Sending color: {:06X}", color);
    
        // you send a 24 bit packet to WS2812 with each `send` being a pair of high and low pulses
        /* From the datasheet
        T0H 0 code ,high voltage time 0.4us ±150ns 
        T1H 1 code ,high voltage time 0.8us ±150ns
        T0L 0 code , low voltage time 0.85us ±150ns
        T1L 1 code ,low voltage time 0.45us ±150ns
        */
        // use the ticks per second of the RMT driver
        let ticks_hz = driver.counter_clock()?;
        // not sure why it needs to be a reference
        let T0H = Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(400))?;
        let T1H = Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(800))?;
        let T0L = Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(850))?;
        let T1L = Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(450))?;
    
        // create fixed length signal of 24 bits, 8 bits per color, to send WS2812
        let mut signal = FixedLengthSignal::<24>::new();


    // loop through each bit and send the pulse sequence
    // MSB first
    for i in (0..24).rev() {
        // bit mask for the current color bit
        let bit_mask = 2u32.pow(i);
        // bit boolean, true if 1, false if 0
        let bit_bool = (color & bit_mask) != 0; 
        // create tuple pairs for both conditions
        let (high, low) = if bit_bool {
            (T1H, T1L)
        }
        else {
            (T0H, T0L)
        };
        // set the signal per bit, decrementing size
        signal.set(23 - i as usize, &(high, low))?;
    }
    driver.start_blocking(&signal)?;

    Ok(())
}

fn main() -> Result<()> {   
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();

    // manual RMT setup
    // let freq = Rate
    let pin = p.pins.gpio2;
    let channel = p.rmt.channel0;
    let mut tx = TxRmtDriver::new(
        channel,
        pin,
        &TransmitConfig::new().clock_divider(2), // 160MHz / 2
    )?;
    

    log::info!("Hello, world!");
    loop{
        // created shifted 24 bit RGB values
        let green = (0xFF) << 16 | 0x00 << 8 | 0x00;
        let red = 0x00 << 16 | (0xFF) << 8 | 0x00;
        let blue = 0x00 << 16 | 0x00 << 8 | (0xFF);

        // send the colors, RMT needs a minimum ~80us delay between frames for latching
        send_frame(green, &mut tx)?;
        FreeRtos::delay_ms(1000);
        send_frame(red, &mut tx)?;
        FreeRtos::delay_ms(1000);
        send_frame(blue, &mut tx)?;
        FreeRtos::delay_ms(1000);
    }
}

use anyhow::Result;
use core::time::Duration;
use esp_idf_hal::{
    gpio::OutputPin,
    peripheral::Peripheral,
    rmt::{config::TransmitConfig, FixedLengthSignal, PinState, Pulse, RmtChannel, TxRmtConfig},
};

// pub use meaning reexport the RGB struct in main
pub use rgb::RGB;

// starting with no lifetime, rule of thumb for <'d> stands for device/driver
pub struct WS2812RMT {
    tx_rmt: TxRmtDriver
}

// create implementation for the WS2812RMT
impl WS2812RMT {
    pub fn new (

        // led and channel implement Peripheral, with the trait restrictions of OutputPin and RmtChannel
        led: impl Peripheral<P = impl OutputPin>,
        channel: impl Peripheral<P = impl RmtChannel>

    ) -> Result<Self> {
        let config = TransmitConfig::new().clock_divider(2); // 160MHz / 2
        let tx = TxRmtDriver::new(channel, led, &config)?;
        Ok(Self { tx_rmt: tx})
    } 

    pub fn set_pixel(&mut self, rgb: RGB8) -> Result<()> {

    }
}


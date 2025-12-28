use std::sync::{Arc, Mutex};

use esp_idf_hal::{gpio::{AnyOutputPin, InputPin, OutputPin}, i2c::{I2C0, I2c, I2cDriver}, peripheral::Peripheral, temp_sensor};
use shtcx::{ShtCx, sensor_class::Sht2Gen};
use esp_idf_hal::units::KiloHertz;

use crate::utils::i2c;


pub struct I2CManager {
    // the main temp sensor, clonable, lockable
    pub temp_sensor: Arc<Mutex<ShtCx<Sht2Gen, I2cDriver<'static>>>>,
}

impl I2CManager {
    pub fn new(
        sda: impl OutputPin + InputPin + 'static,
        scl: impl OutputPin + InputPin + 'static,
        i2c: I2C0,
    ) -> Self {
        let config = esp_idf_hal::i2c::I2cConfig::new()
            .baudrate(KiloHertz(100).into());
        let i2c = I2cDriver::new(i2c, sda, scl, &config).unwrap();
        let temp_sensor = Arc::new(Mutex::new(shtcx::shtc3(i2c)));
        Self { temp_sensor }
    }
}
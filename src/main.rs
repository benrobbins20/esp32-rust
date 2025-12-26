use std::{default, sync::{Arc, Mutex}, time::Duration};
use anyhow::{Result, bail};
use esp_idf_svc::{eventloop::EspSystemEventLoop, hal::prelude::Peripherals, http::{client::EspHttpConnection, server::EspHttpServer}, mqtt::client::MqttClientConfiguration, wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi}};
use esp_idf_hal::{delay::FreeRtos, gpio::Pins, i2c::{I2C0, I2cConfig, I2cDriver}, io::{EspIOError, Read}, peripheral::{self, Peripheral}, prelude::*, rmt::{FixedLengthSignal, PinState, Pulse, PulseTicks, TxRmtDriver, config::TransmitConfig}, units::Hertz};
use rgb::RGB8;
use embedded_svc::http::client::Client;
use embedded_svc::http::Method;
use embedded_svc::wifi::Configuration as WifiConfiguration;
use esp_idf_svc::http::server::Configuration as HttpConfiguration;
use shtcx::{ShtCx, sensor_class::Sht2Gen, shtc3};
use uuid::Uuid;
use uuid;

mod utils; 
use utils::wifi::WifiManager;
use utils::http::HttpConn;
use utils::rgb::RMTDriver;

use crate::utils::rgb::Color;
// bring in secrets
// cfg.toml generates this struct as SHOUTY_SNAKE const
#[toml_cfg::toml_config]
pub struct Config {
    #[default("test")]
    wifi_ssid: &'static str,
    #[default("test")]
    wifi_password: &'static str,
    #[default("test.mosquitto.org")]
    mqtt_broker: &'static str,
    #[default("bob")]
    mqtt_user: &'static str,
    #[default("bob")]
    mqtt_pass: &'static str
}

// const UUID: &'static str = get_uuid::uuid();

// struct for i2c
struct I2CDev {
    sda: esp_idf_hal::gpio::Gpio10,
    scl: esp_idf_hal::gpio::Gpio8,
    i2c: esp_idf_hal::i2c::I2C0
}
impl I2CDev {
    fn new(
        sda: esp_idf_hal::gpio::Gpio10,
        scl: esp_idf_hal::gpio::Gpio8,
        i2c: esp_idf_hal::i2c::I2C0
    ) -> Self {
        I2CDev {
            sda,
            scl,
            i2c
        }
    }
}


fn configure_i2c(dev: I2CDev) -> Arc<std::sync::Mutex<ShtCx<Sht2Gen, I2cDriver<'static>>>> {
    let config = I2cConfig::new()
        .baudrate(100.kHz().into());
    
    // return driver 
    let i2c = I2cDriver::new(dev.i2c, dev.sda, dev.scl, &config).unwrap();
    let i2c = Arc::new(Mutex::new(shtc3(i2c)));
    i2c
}

fn main() -> Result<()> {   
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let pins = p.pins;
    let i2c = p.i2c0;

    // toml config
    let config = CONFIG;


 
    let mut wifi = WifiManager::new(
        config.wifi_ssid, 
        config.wifi_password, 
        p.modem, 
        sysloop.clone())?;
            
    wifi.connect()?;

    let i2cdev = I2CDev::new(pins.gpio10, pins.gpio8, i2c);
    let temp = configure_i2c(i2cdev);
    let mut ts = temp.clone();

    // ts
    //     .lock()
    //     .unwrap()
    //     .start_measurement(shtcx::PowerMode::NormalMode)
    //     .unwrap();

    // addressable WS2812 LED setup via RMT
    let mut driver = RMTDriver::new(
        p.rmt.channel0,
        pins.gpio2,
    )?;
    driver.clear()?;
    
    // GRB remember 
    let green = Color::try_from("FF0000")?;
    let red = Color::try_from("00FF00")?;
    let blue = Color::try_from("0000FF")?;


    driver.set_rgb(blue)?;
    FreeRtos::delay_ms(1000);
    driver.clear()?;
    


    // let mut http_conn = HttpConn::new()?;
    // http_conn.http_get("http://darksouls.wikidot.com/")?; // bug, seems to lock up because its so large
    // http_conn.http_get("http://neverssl.com")?;
    // http_get("http://info.cern.ch/")?;


    let uuid = Uuid::new_v4().to_string();
    log::info!("Device UUID: {}", uuid);
    let mqtt_cfg = MqttClientConfiguration::default();

    // likely using test.mosquitto or other public broker, plain mqtt://<> url if no creds
    let broker_url = if config.mqtt_user != "" {
        format!(
            "mqtt://{}:{}@{}",
            config.mqtt_user, config.mqtt_pass, config.mqtt_broker
        )
    } else {
        format!("mqtt://{}", config.mqtt_broker)
    };

    let mut mqtt_client = esp_idf_svc::mqtt::client::EspMqttClient::new_cb(
        &broker_url,
        &mqtt_cfg,
    )?;




    loop{

        // temp
        //     .lock()
        //     .unwrap()
        //     .start_measurement(shtcx::PowerMode::NormalMode)
        //     .unwrap();
        FreeRtos::delay_ms(1000);
    }
}

fn html_template(content: String) -> String {
    format!(
        "<!DOCTYPE html>
        <html>
            <head>
                <title>ESP32-RS SHTC3 Sensor</title>
            </head>
            <body>
                <h1>ESP32-RS SHTC3 Sensor Data</h1>
                <p>Temperature and Humidity data will be displayed here.</p>
                <div>{}</div>
            </body>
        </html>",
        content
    )
}


fn temperature(content: String) -> String {
    html_template(format!("temp: {}", content))
}

fn index() -> String {
    html_template("hello from ESP32".to_string())
}
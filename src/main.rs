use std::{default, sync::{Arc, Mutex}, time::Duration};
use anyhow::{Result, bail};
use esp_idf_svc::{eventloop::EspSystemEventLoop, hal::prelude::Peripherals, http::{client::EspHttpConnection, server::EspHttpServer}, mqtt::client::MqttClientConfiguration, wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi}};
use esp_idf_hal::{delay::{self, Delay, FreeRtos}, gpio::{OutputPin, Pins}, i2c::{I2C0, I2cConfig, I2cDriver}, io::{EspIOError, Read}, peripheral::{self, Peripheral}, prelude::*, rmt::{FixedLengthSignal, PinState, Pulse, PulseTicks, TxRmtDriver, config::TransmitConfig}, units::Hertz};
use rgb::RGB8;
use embedded_svc::http::client::Client;
use embedded_svc::http::Method;
use embedded_svc::wifi::Configuration as WifiConfiguration;
use esp_idf_svc::http::server::Configuration as HttpConfiguration;
use shtcx::{ShtCx, sensor_class::Sht2Gen, shtc3};
use uuid::Uuid;
use uuid;

// bring in utils library
mod utils; 
use utils::wifi::WifiManager;
use utils::http::HttpConn;
use utils::rgb::RMTDriver;
use utils::i2c::{I2CManager};

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

// const UUID: String = Uuid::new_v4().to_string();



fn main() -> Result<()> {   
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let pins = peripherals.pins;

    // toml config
    let config = CONFIG;

    let mut wifi = WifiManager::new(
        config.wifi_ssid, 
        config.wifi_password, 
        peripherals.modem, 
        sysloop.clone())?;
            
    wifi.connect()?;

    let i2c = I2CManager::new(pins.gpio10, pins.gpio8, peripherals.i2c0);
    // let ts = i2c.temp_sensor.clone();

    // // start i2c
    // ts
    //     .lock()
    //     .unwrap()
    //     .start_measurement(shtcx::PowerMode::NormalMode)
    //     .unwrap();

    // addressable WS2812 LED setup via RMT
    let mut driver = RMTDriver::new(
        peripherals.rmt.channel0,
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
        move |_msg| {
            // closure body, no callback
        },
    )?;

    // empty payload
    let payload: &[u8] = &[];
    mqtt_client.enqueue(
        format!("{}/hello", uuid).as_str(), 
        esp_idf_svc::mqtt::client::QoS::AtLeastOnce, 
        true, 
        &payload)?;

    // main loop    
    loop{
        // this keeps i2c locked
        let temp = i2c.temp_sensor
            .lock()
            .unwrap()
            .measure_temperature(shtcx::PowerMode::NormalMode, &mut FreeRtos)
            .unwrap()
            .as_degrees_celsius();
        // mqtt will send float as 'big endian network bytes'
        log::info!("Temperature: {:?} °C Bytes: {:?}", temp, &temp.to_be_bytes());
        

        // publish temperature to mqtt topic
        mqtt_client.enqueue(
            format!("test").as_str(),
            esp_idf_svc::mqtt::client::QoS::AtLeastOnce,
            false,
            &temp.to_be_bytes(),
        )?;

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
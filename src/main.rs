use log::info;
use rand::Rng;
use rumqttc::{Client, MqttOptions, Packet, QoS};
use uuid::Uuid;
use std::{error::Error, thread, time::Duration};
use anyhow::{Result};
use rgb::{Rgba};
use log::log;


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
impl From<[u8; 3]> for Color {
    fn from(value: [u8; 3]) -> Self {
        Self {
            r: value[0],
            g: value[1],
            b: value[2],
        }
    }
}

fn color_task(client: Client) {
    let _ = thread::spawn(move || {
        let mut rng = rand::rng();
        loop {
            let r: u8 = rng.random();
            let g: u8 = rng.random();
            let b: u8 = rng.random();
            let payload = [r,g,b];
            client
                .publish("color", QoS::AtLeastOnce, false, &payload)
                .unwrap();

            println!("Published color: R={}, G={}, B={}", r, g, b);
            thread::sleep(Duration::from_millis(500));
        }
    });
}

fn main() {
    

    let uuid = Uuid::new_v4().to_string();
    let host = "192.168.86.16";
    let port = 1883;
    let mut mqttoptions = MqttOptions::new(uuid, host, port);


    // mqttoptions.set_credentials("bob", "bob");
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (client, mut connection) = Client::new(mqttoptions, 25);

    client
        .subscribe("temp", QoS::AtMostOnce)
        .unwrap();
    
    color_task(client);

    // poll for temperature data, try_into float
    for (_, notification) in connection.iter().enumerate() {
        // bind message to variable if a publish packet
        if let Ok(rumqttc::Event::Incoming(Packet::Publish(packet))) = notification {

            // declare payload is an array (vector) of bytes, try convert into sized array
            let temp_bytes: &[u8] = &packet.payload;
            let temp_bytes: Result<[u8; 4], _> = temp_bytes.try_into();
            
            // convert to single precision float and print
            if let Ok(temp_bytes) = temp_bytes {
                let temp: f32 = f32::from_be_bytes(temp_bytes);
                println!("Temperature: {:.2} C", temp);
                
            }
        }
    }
}

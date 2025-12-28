use rand::Rng;
use rumqttc::{Client, MqttOptions, Packet, QoS};
use uuid::Uuid;
use std::{error::Error, thread, time::Duration};

// generic mqtt client, aarch64 darwin

fn main() {

    let uuid = Uuid::new_v4().to_string();
    let host = "192.168.8.1";
    let port = 1883;
    let mut mqttoptions = MqttOptions::new(uuid, host, port);


    mqttoptions.set_credentials("", "");
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (client, mut connection) = Client::new(mqttoptions, 25);

    client
        .subscribe("test", QoS::AtMostOnce)
        .unwrap();
    
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

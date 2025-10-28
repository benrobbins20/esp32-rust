use std::{default, time::Duration};
use anyhow::{bail, Result};
use esp_idf_svc::{eventloop::EspSystemEventLoop, hal::prelude::Peripherals, http::{client::EspHttpConnection}, wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi}};
use esp_idf_hal::{delay::FreeRtos, io::Read, peripheral::{self, Peripheral}, rmt::{config::TransmitConfig, FixedLengthSignal, PinState, Pulse, PulseTicks, TxRmtDriver}};
use rgb::RGB8;
use embedded_svc::http::client::Client;
use embedded_svc::http::Method;

// bring in secrets
// cfg.toml generates this struct as SHOUTY_SNAKE const
#[toml_cfg::toml_config]
pub struct WifiConfig {
    #[default("test")]
    wifi_ssid: &'static str,
    #[default("test")]
    wifi_password: &'static str
}

fn send_frame(color: u32, driver: &mut TxRmtDriver) -> Result<()> {

        log::info!("Sending color: {:06X}", color);
    
        // you send a 24 bit packet to WS2812 with each bit being set in fixed length buffer
        // each bit is sent as a pair of high/low pulses in a pre-defined interval
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

// 
fn idf_wifi(
    ssid: &str,
    password: &str,
    modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop
) -> Result<Box<EspWifi<'static>>> {
    let mut auth_method = AuthMethod::WPA2Personal;

    // if ssid.is_empty() {
    //     bail!("ssid not found")
    // }
    // if password.is_empty() {
    //     bail!("password not found")
    // }

    // ASYNC wifi instance
    let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), None).unwrap();
    // wrap in blocking connect so you don't need to poll/await until connected
    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop).unwrap();
    wifi.set_configuration(&Configuration::Client(esp_idf_svc::wifi::ClientConfiguration::default())).unwrap(); // check the defaults

    log::info!("starting wifi...");
    wifi.start().unwrap();

    log::info!("scanning for ap's");
    let ap_list = wifi.scan().unwrap();

    // print all ap's
    for ap in &ap_list {
        log::info!("found ap: ssid {:?}, channel {}, auth {:?}", ap.ssid, ap.channel, ap.auth_method);
    }

    // scan returns a vector of ap info structs, iterate through and match the ssid
    let ap = ap_list.into_iter().find(|found_ap| found_ap.ssid == ssid);

    // assign the channel after reading broadcasted ap info
    let channel = if let Some(ap) = ap {
        log::info!("found ap {:?}, channel {}", ssid, ap.channel);
        Some(ap.channel)
    }
    else {
        log::error!("could not find ap {}", ssid);
        None
    };

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid
            .try_into()
            .expect("failed parsing ssid"),
        password: password
            .try_into()
            .expect("failed to parse password"),
        channel,
        auth_method,
        ..Default::default() // will assign the remaining parameters as default
    })).unwrap();

    // connect and get an IP address
    log::info!("connecting to {}", ssid);
    wifi.connect().unwrap();
    wifi.wait_netif_up().unwrap();
    let ip_info = wifi.wifi().sta_netif().get_ip_info().unwrap();
    log::info!("DHCP info {:?}", ip_info);

    // return the async instance of EspWifi after using the wrapped instance to connect
    Ok(Box::new(esp_wifi))
}


fn http_get(url: impl AsRef<str>) -> Result<()> {
    // load default EspWifiConnection
    let conn_cfg = esp_idf_svc::http::client::Configuration::default();
    let conn = EspHttpConnection::new(&conn_cfg)?;
    
    let mut client = Client::wrap(conn);

    let headers = [("accept", "text/plain")];
    // create a request
    let req = client.request(Method::Get, url.as_ref(), &headers)?;
    // send request and store response
    let resp = req.submit()?;
    let status = resp.status();
    log::info!("Response status: {}", status);

    // match/map the status code to a behavior
    match status {
        // success status codes
        200..=299 => {
            // buffer for recv chunks
            let mut buf = [0u8; 512];
            // offset to track which 8 byte `buffer address` to write to 
            let mut offset = 0;
            // counter to determine len
            let mut total = 0;
            let mut reader = resp;

            loop {
                // read up to a 512 byte chunk, read -> Result<usize> = size
                if let Ok(size) = Read::read(&mut reader, &mut buf[offset..]) {
                    // response empty or data exhausted
                    if size == 0 {
                        break;
                    }
                    total += size; // add the chunk to total
                    let size_plus_offset = size + offset;
                    // try to convert all the bytes in the chunk up to whatever is in the chunk
                    match str::from_utf8(&buf[..size_plus_offset]) {
                        Ok(text) => {
                            log::info!("Received chunk: {}", text);
                            // reset offset
                            offset = 0;
                        }

                        // error if from_utf8 fails, attempt to reconstruct the data
                        Err(e) => {
                            // create a boundary between where data is readable and not, SURPRINGSLY EASY
                            let valid_to = e.valid_up_to();
                            // unchecked method is unsafe but should all be printable chars
                            unsafe {
                                print!("{}", str::from_utf8_unchecked(&buf[..valid_to]));
                            }
                            // copy the remaining invalid data to the beginning of the buffer
                            buf.copy_within(valid_to.., 0);
                            // set the offset to after the size of the invalid data and continue reading
                            offset = size_plus_offset - valid_to;
                        }
                    }
                }
            }

            log::info!("Total bytes received: {}", total);


        }
        // anything else _ => bail
        _ => 
            bail!("HTTP request failed: {}", status),
    }



    Ok(())
}


fn main() -> Result<()> {   
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();

    // addressable WS2812 LED setup via RMT
    let pin = p.pins.gpio2;
    let channel = p.rmt.channel0;
    let mut tx = TxRmtDriver::new(
        channel,
        pin,
        &TransmitConfig::new().clock_divider(2), // 160MHz / 2
    )?;
    
    let wifi_config = WIFI_CONFIG;

    let _wifi = match idf_wifi(wifi_config.wifi_ssid, wifi_config.wifi_password, p.modem, sysloop){
        Ok(inner) => inner,
        Err(err) =>{
            bail!("Wifi connect failed")
        }
    };

    http_get("http://neverssl.com/")?;

    log::info!("Hello, world!");
    loop{
        // create shifted 24 bit RGB values
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

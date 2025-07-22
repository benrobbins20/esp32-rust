
use std::{str::from_utf8, sync::{Arc, Mutex}, thread::sleep, time::Duration};
use anyhow::Ok;
use esp_idf_hal::{gpio::PinDriver, io::Read, prelude::Peripherals};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::timer::{EspTaskTimerService};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AsyncWifi, PmfConfiguration};
use esp_idf_svc::ping::{Configuration as PingConfiguration, EspPing};
use esp_idf_svc::wifi::EspWifi;
use embedded_svc::wifi::{ClientConfiguration, Configuration as WifiConfiguration, ScanMethod::CompleteScan};
use embedded_svc::wifi::AuthMethod;
use log::info;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::nvs::EspNvsPartition;
use esp_idf_svc::nvs::NvsDefault;
use heapless::String;
use std::iter::Iterator;


use ws2812_esp32_rmt_driver::driver::color::LedPixelColorGrbw32;
use ws2812_esp32_rmt_driver::{LedPixelEsp32Rmt, RGBW8};
use smart_leds_trait::{SmartLedsWrite, White};




// this is pissed but its still the correct way to do it. 
// env variables are stored in the shell environment and are loaded at compile time
const SSID: &str = env!("RUST_ESP32_STD_DEMO_WIFI_SSID");
const PASS: &str = env!("RUST_ESP32_STD_DEMO_WIFI_PASS");

// color struct and parse 
#[derive(Debug)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}
impl TryFrom<&str> for Color {
    type Error = anyhow::Error;
    
    fn try_from(input: &str) -> anyhow::Result<Self> {
        Ok(Color {
            r: u8::from_str_radix(&input[0..2], 16)?,
            g: u8::from_str_radix(&input[2..4], 16)?,
            b: u8::from_str_radix(&input[4..6], 16)?
        })
    }
}


fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    // log::info!("Hello, world!");
    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take().unwrap();
    let timer_service = EspTaskTimerService::new().unwrap();

    // log the result of this
    let nvs = Some(EspDefaultNvsPartition::take().unwrap());

    // having an issue with the unwrap() panic at the end
    let wifi_result = wifi(peripherals.modem, 
        sysloop, 
        nvs, 
        timer_service).unwrap();
    log::info!("Wifi started: {:?}", wifi_result.wifi().sta_netif().get_ip_info());


    // Create http server endpoint, set default values like port 80, 443, timeout, etc.
    let mut server = EspHttpServer::new(&Default::default()).unwrap();
    
    // led - gpio 21 on the xaio esp32s3, gpio 2 for esp32-c3-devkit-RUST-1
    // let led = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio2).unwrap()));
    
    // let mut led = WS2812RMT::new(peripherals.pins.gpio2, peripherals.rmt.channel0).unwrap();
    // led.set_pixel(RGB8::new(50, 50, 0)).unwrap();

    let led_pin = peripherals.pins.gpio2;
    let channel = peripherals.rmt.channel0;
    let ws2812 = LedPixelEsp32Rmt::<RGBW8, LedPixelColorGrbw32>::new(channel, led_pin).unwrap();
    
    // wrap the rmt driver in arc mutex to allow shared access
    let ws2812 = Arc::new(Mutex::new(ws2812));
    // clone is a pointer to the driver
    let ws2812_handle = ws2812.clone();

    
    server.fn_handler("/color", embedded_svc::http::Method::Post, move |mut req| {
        // lambda function/closure for request

        // buffer for post data
        let mut buffer = [0u8;6];
        req.read_exact(&mut buffer)?;
        let color: Color = from_utf8(&buffer)?.try_into()?;
        println!("Color: {:?}", color);
        let mut response = req.into_ok_response()?;
        response.write("ESP32 Web Server".as_bytes())?;
        

        // led.lock().unwrap().toggle()?;

        // block until pointer mutex can be acquired, create mutable reference local to the closure
        let mut ws2812 = ws2812_handle.lock().unwrap();

        let pixels = std::iter::repeat(RGBW8 {r: color.r, g: color.g, b: color.b, a: White(30)}).take(1);
        ws2812.write(pixels).unwrap();

        Ok(())
    }).unwrap();

    loop {
        sleep(Duration::from_secs(1));
    }
}

pub fn wifi (
    // modem implements the Peripheral trait, P type must be esp_idf_hal::modem::Modem type
    modem: impl Peripheral<P = esp_idf_hal::modem::Modem> + 'static, // static lifetime requirement
    sysloop: EspSystemEventLoop, // use the esp event loop
    nvs: Option<EspNvsPartition<NvsDefault>>, // non-volatile storage for wifi config/keys
    timer_service: EspTaskTimerService, // timer service for async
    // wifi function returns an AsyncWifi instance, anyhow is used for error handling
    // if asyncwifi creation fails, anyhow helps to define the error
) -> anyhow::Result<AsyncWifi<EspWifi<'static>>> {
    use futures::executor::block_on;
    // wrap the Espwifi with a generic AsyncWifi instance
    let mut wifi = AsyncWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), nvs)?,
         sysloop, 
         timer_service.clone(),
        )?;

    // pass a reference of this function to connect_wifi() fn
    block_on(connect_wifi(&mut wifi))?;
    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    println!("Wifi dhcp info: {:?}", ip_info);

    EspPing::default().ping(ip_info.subnet.gateway, &PingConfiguration::default())?;
    
    Ok(wifi)
}
async fn connect_wifi(wifi: &mut AsyncWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    
    // allocate memory for SSID and PASS env variables 
    let mut ssid: String<32> = String::new();
    ssid.push_str(SSID).unwrap();
    let mut password: String<64> = String::new(); // has to be <64> per the configuration
    password.push_str(PASS).unwrap();

    let wifi_configuration: WifiConfiguration = WifiConfiguration::Client(ClientConfiguration{
        ssid,
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password, 
        channel: None,
        scan_method: CompleteScan(Default::default()),
        pmf_cfg: PmfConfiguration::default()
    });
    
    wifi.set_configuration(&wifi_configuration)?;
    wifi.start().await?;
    info!("Wifi started");
    wifi.connect().await?;
    info!("Wifi connected");
    wifi.wait_netif_up().await?;
    info!("Wifi netif up");
    Ok(())
}

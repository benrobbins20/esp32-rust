
use std::{collections::HashMap, sync::{Arc, Mutex}, thread::sleep, time::Duration};
use anyhow::Ok;
use esp_idf_hal::{gpio::PinDriver, ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver}, prelude::Peripherals, units::FromValueType};
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
use std::str::from_utf8;
use embedded_svc::http::Method::Get;
use accel_stepper::{Driver, OperatingSystemClock, SystemClock};
use esp_idf_hal::delay::FreeRtos;



// bring in stepper file
mod stepper;
use stepper::Stepper;

// this is pissed but its still the correct way to do it. 
// env variables are stored in the shell environment and are loaded at compile time
const SSID: &str = env!("RUST_ESP32_STD_DEMO_WIFI_SSID");
const PASS: &str = env!("RUST_ESP32_STD_DEMO_WIFI_PASS");

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
    
    // led - gpio 21 on the xaio esp32s3, wrap the gpio in PinDriver
    let led = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio21).unwrap()));
    
    // setup servo timer
    // let servo_timer = peripherals.ledc.timer1;
    // let servo_driver = LedcTimerDriver::new(servo_timer, &TimerConfig::new().frequency(50_u32.Hz()).resolution(esp_idf_hal::ledc::Resolution::Bits14)).unwrap();
    // let servo = Arc::new(Mutex::new(LedcDriver::new(peripherals.ledc.channel3, servo_driver, peripherals.pins.gpio1).unwrap()));

    
    let mut stepper = stepper::Stepper::new(
        peripherals.pins.gpio1,
        peripherals.pins.gpio2,
        peripherals.pins.gpio3,
        peripherals.pins.gpio4
    );


    let mut driver = Driver::default();
    driver.set_max_speed(500.0);
    driver.set_acceleration(100.0);
    let driver = Arc::new(Mutex::new(driver));
    let http_driver = driver.clone();

    let clock = OperatingSystemClock::new();

    // 50Hz, 1 cycle in 20 ms
    // duty cycles is how many ticks per 20ms, with 14 bit resolution, 

    // standard sweep 
    // 5% ~819/16383 1ms
    // 10% ~1638/16383 2ms

    // wide sweep
    // 2.5% ~409/16383 .5ms
    // 12.5% ~2048/16383 2.5ms

    // setup Stepper struct for simple stepping
    // let stepper::Stepper::

    // let max_duty = servo.lock().unwrap().get_max_duty();
    // let min = max_duty / 40; // 2.5%
    // let max = max_duty / 8; // 12.5%

    fn interpolate(angle: u32, min: u32, max: u32) -> u32 {
        let mut total;
        // total bit range is max - min
        total = max - min;
        // map degrees to bits, ~9 bits per degree
        total /= 180; 
        // desired angle * bits per degree
        total *= angle;
        // offset desired angle by the minimum duty cycle
        total += min;
        
        total
    }

    // servo http handler
    server.fn_handler("/servo", embedded_svc::http::Method::Post, move |mut req| {
        let mut buffer = [0_u8;6];
        let bytes_read = req.read(&mut buffer).unwrap();
        let angle_string = from_utf8(&buffer[0..bytes_read]).unwrap();
        let angle: u32 = angle_string.parse().unwrap();

        // servo.lock().unwrap().set_duty(interpolate(angle, min, max)).unwrap();

        // led.lock().unwrap().toggle().unwrap();
        Ok(())
    }).unwrap();

    // http stepper handler
    server.fn_handler("/stepper", Get, move |mut req| {
        let uri = req.uri();
        let parts = uri.split_once("?").map(|(_, query_params)| query_params).unwrap_or(""); 
        let params: HashMap<&str, &str> = parts.split("&").filter_map(|param| param.split_once("=")).collect();
        let command: i64 = params.get("command").unwrap_or(&"").parse().unwrap_or(0);

        http_driver.lock().unwrap().move_by(command);
        Ok(())
    }).unwrap();

    // must poll fast enough to be within step duration
    loop {
        driver.lock().unwrap().poll(&mut stepper, &clock);
        FreeRtos::delay_ms(2);
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

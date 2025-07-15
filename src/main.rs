
use std::{thread::sleep, time::Duration, sync::{Mutex, Arc}};


use anyhow::Ok;
use esp_idf_hal::{gpio::PinDriver, prelude::Peripherals};
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

    server.fn_handler("/", embedded_svc::http::Method::Get, move |req| {
        // lambda function/closure for request
        let mut response = req.into_ok_response().unwrap();
        response.write("ESP32 Web Server".as_bytes()).unwrap();
        led.lock().unwrap().toggle().unwrap();
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

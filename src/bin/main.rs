#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::net::Ipv4Addr;

use alloc::boxed::Box;
use alloc::string::ToString;
use embassy_executor::Spawner;
use embassy_net::IpAddress;
use embassy_net::icmp::PacketMetadata;
use embassy_net::icmp::ping::PingManager;
use embassy_net::{Runner, StackResources, icmp::IcmpSocket};
use embassy_time::{Duration, Timer, Instant};
use esp_hal::ledc::timer;
use esp_hal::xtensa_lx::singleton;
use esp_hal::{Async, Blocking};
use esp_hal::gpio::{Level, OutputConfig};
use esp_hal::spi::{master::{Spi,Config as SpiConfig}, Mode as SpiMode};
use esp_hal::time::Rate;
use esp_hal::{clock::CpuClock, gpio::Output};
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_alloc::HEAP;
use esp_wifi::wifi::event::StaDisconnected;
use log::{info, debug, error};
use esp_println::{println, print};
extern crate alloc;
use rgb::{RGB};
use esp_wifi::wifi::{ClientConfiguration, Configuration as WifiConfiguration, WifiController, WifiDevice};
use static_cell::{make_static, StaticCell};


static WIFI_INIT: StaticCell<esp_wifi::EspWifiController<'static>> = StaticCell::new();
static STACK: StaticCell<embassy_net::Stack<'static>> = StaticCell::new(); 
static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

#[toml_cfg::toml_config]
pub struct Config {
    #[default("test")]
    wifi_ssid: &'static str,
    #[default("test")]
    wifi_password: &'static str,
    #[default("test.mosquitto.org")]
    mqtt_broker: &'static str,
    #[default("")]
    mqtt_user: &'static str,
    #[default("")]
    mqtt_pass: &'static str
}


// no_std requires a panic handler, default to non-divergent (!) infinite loop 
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

fn get_rgb_frame(pos: u8) -> [u8;12] {
    // array of bytes to store RGB values, split this up into RGB struct return
    let rgb_val: [u8;3];


    // initially starting 255, 0, 0 (red)
    if pos < 85 {
        print!("\rR->B");                // return print cursor to beginning of line for less print noise
        let inner_pos = pos;       // outer already at zero but define a local position counter
        let r = (255 - (inner_pos * 3));   // red decreases to 0; 0:84
        let g = 0;                       // green is off
        let b = (inner_pos * 3);           // blue increases to 255
        rgb_val = [r, g, b];
    }
    else if pos < 170 {
        print!("\rB->G");
        let inner_pos = pos - 85;   // continue from 0 again (85-85)
        let r = 0;                      
        let g = (inner_pos * 3) as u8;            // green grows from 0:255
        let b = (255 - (inner_pos * 3)) as u8;    // blue shrinks from 255:0
        rgb_val = [r, g, b];
    }
    else {
        print!("\rG->R");
        let inner_pos = pos - 170;    // zero the counter
        let r = (inner_pos * 3) as u8;              // red grows
        let g = (255 - (inner_pos * 3)) as u8;      // green shrinks
        let b = 0;
        rgb_val = [r, g, b];
    }
    let start_frame: [u8;4] = [0x00, 0x00, 0x00, 0x00]; 
    let rgb: [u8;4] = [0xE8, rgb_val[0], rgb_val[1], rgb_val[2]];
    let end_frame: [u8;4] = [0xFF, 0xFF, 0xFF, 0xFF];
    let mut full_frame: [u8;12] = [0;12];
    full_frame[0..4].copy_from_slice(&start_frame);
    full_frame[4..8].copy_from_slice(&rgb);
    full_frame[8..12].copy_from_slice(&end_frame);
    full_frame
}

#[embassy_executor::task]
async fn colorwheel(mut spi: Spi<'static, Blocking>) {
    let mut pos = 0u8;
    loop {
        let frame = get_rgb_frame(pos);
        pos = (pos + 1) % 255; // loop back to zero after one full cycle
        spi.write(&frame).unwrap();
        Timer::after(Duration::from_millis(50)).await; // delay for visibility
    }
}

// connect to wifi
#[embassy_executor::task]
async fn conn(mut cont: WifiController<'static>) {
    info!("Starting wifi connection task");
    let config = WifiConfiguration::Client(ClientConfiguration {
        ssid: CONFIG.wifi_ssid.into(),
        password: CONFIG.wifi_password.into(),
        ..Default::default()
    });

    // 
    cont.set_configuration(&config).unwrap();
    cont.start().unwrap();
    info!("Wifi controller started");

    // scan for ap's once during initial startup
    info!("Scanning for WiFi networks...");
    match cont.scan_with_config_async(Default::default()).await {
        Ok(result) => {
            info!("Scan complete. Found {} networks:", result.len());
            for ap in result {
                info!("{:?}", ap)
            }
        }
        Err(e) => {
            info!("Scan failed: {:?}", e);
        }
    }

    // reconnection handling
    loop {
        info!("Connecting to {}", CONFIG.wifi_ssid);
        match cont.connect_async().await {
            Ok(_) => {
                info!("Connected");
                cont
                    .wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected)
                    .await;
                // print the error after this event happens in the Ok branch
                error!("Disconnected from WiFi. Attempting to reconnect...");
            }
            Err(e) => {
                info!("Connection failed: {:?}. Retrying in 5 seconds...", e);
                Timer::after(Duration::from_secs(5)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn stack_runner(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await;
}

// ping mqtt broker
#[embassy_executor::task]
async fn ping(stack: &'static embassy_net::Stack<'static>) {
    // buffers
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buf = [0u8; 512];
    let mut tx_buf = [0u8; 512];

    // wait for ipv4 
    while stack.config_v4().is_none() {
        Timer::after(Duration::from_millis(500)).await;
    }
    info!("Got IP address: {:?}", stack.config_v4().unwrap().address);
    info!("Gateway: {:?}", stack.config_v4().unwrap().gateway);

    let target = IpAddress::from(Ipv4Addr::new(192, 168, 86, 16));
    let mut ping = PingManager::new(stack.clone(), &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    let mut seq: u16 = 0;
    let params = embassy_net::icmp::ping::PingParams::new(target);

    loop {
        seq = seq.wrapping_add(1);
        info!("Pinging {} with seq={}", target, seq);
        match ping.ping(&params).await {
            Ok(rtt) => info!("Reply from {}: seq={} time={} ms", target, seq, rtt.as_millis()),
            Err(e) => error!("Ping error: {:?}", e),
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0 - created by esp-generate 

    // create global logger which can redirect log::info!()
    esp_println::logger::init_logger(log::LevelFilter::Debug); // log everything
    info!("Hello world! - info");
    println!("Hello world! - println");

    // straight up going to town on debug
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    debug!("{:?}", config.cpu_clock());
    let peripherals = esp_hal::init(config); // print debug stuff after initialized
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // print stats then create a Box to test
    println!("{}", HEAP.free());
    let test_buf = Box::new([0u8; 1024]); // 1KB of junk
    println!("{}", HEAP.stats()); // Internal | ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Used: 1% (Used 1024 of 73728, free: 72704)
    drop(test_buf);
    // println!("{}", HEAP.free()); it worked..  

    // create and assign all peripherals
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);
    let mut rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    // let wifi_init = esp_wifi::init(timer1.timer0, rng);
    let wifi_init = WIFI_INIT.init(esp_wifi::init(timer1.timer0, rng).unwrap());
    let (wifi_controller, interfaces) = esp_wifi::wifi::new(wifi_init, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");

    // embassy now owns the SYSTIMER alarm0
    let start = Instant::now(); // embassy time method
    debug!("Uptime: {} ms", start.as_millis());

    // random hex number
    let rnd_check = rng.random();
    debug!("rng_hex=0x{:08x}", rnd_check);

    // random decimal number
    let rnd_check2 = rng.random();
    debug!("rng_dec=0d{}", rnd_check2);

    // wifi debug
    debug!("wifi started: {:?}", wifi_controller.is_started());
    debug!("wifi capabilities: {:?}", wifi_controller.capabilities());
    let mac = interfaces.sta.mac_address();
    debug!("sta mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]); 

    // network stack
    let net_config = embassy_net::Config::dhcpv4(Default::default());
    let (stack, runner) = embassy_net::new(
        interfaces.sta,
        net_config, 
        RESOURCES.init(StackResources::new()),
        rng.random() as u64,
    );

    let stack = STACK.init(stack);

    let mut spibus = Spi::new(peripherals.SPI2, SpiConfig::default()
        .with_frequency(Rate::from_mhz(60))
        .with_mode(SpiMode::_0)
    )
        .unwrap()
        .with_sck(peripherals.GPIO39)
        .with_mosi(peripherals.GPIO40);

    let _ = spawner;
    spawner.spawn(colorwheel(spibus)).unwrap();
    spawner.spawn(conn(wifi_controller)).unwrap();
    spawner.spawn(stack_runner(runner)).unwrap();
    spawner.spawn(ping(stack)).unwrap();
    
}

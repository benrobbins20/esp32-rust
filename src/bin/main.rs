#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_println::{print, println};
use log::info;
use esp_hal::spi::master::Config as SpiConfig;
use esp_hal::spi::master::Spi;
use esp_hal::spi::Mode as SpiMode;
use embedded_hal_bus::spi::ExclusiveDevice;
extern crate alloc;
esp_bootloader_esp_idf::esp_app_desc!();
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
use rgb::RGB;

// create a static safe position counter for rgb, will wrap automatically
static POSITION: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

// cycle over all positions in a byte and continuously shift 32bit (24bit) RGB values
fn color_shifter() -> RGB<u8> {
    // array of bytes to store RGB values, split this up into RGB struct return
    let rgb_val: [u8;3];
    // everytime function is called, atomic increment static pos counter
    // name this outer position to tell which is global and which is local which is zeroed in each segment
    let outer_pos = POSITION.fetch_add(1, core::sync::atomic::Ordering::SeqCst);

    // initially starting 255, 0, 0 (red)
    if (outer_pos < 85) {
        print!("\rR->B");                // return print cursor to beginning of line for less print noise
        let inner_pos = outer_pos;       // outer already at zero but define a local position counter
        let r = 255 - (inner_pos * 3);   // red decreases to 0; 0:84
        let g = 0;                       // green is off
        let b = inner_pos * 3;           // blue increases to 255
        rgb_val = [r, g, b];
    }
    else if (outer_pos < 170) {
        print!("\rB->G");
        let inner_pos = outer_pos - 85;   // continue from 0 again (85-85)
        let r = 0;                      
        let g = inner_pos * 3;            // green grows from 0:255
        let b = 255 - (inner_pos * 3);    // blue shrinks from 255:0
        rgb_val = [r, g, b];
    }
    else {
        print!("\rG->R");
        let inner_pos = outer_pos - 170;    // zero the counter
        let r = inner_pos * 3;              // red grows
        let g = 255 - (inner_pos * 3);      // green shrinks
        let b = 0;
        rgb_val = [r, g, b];
    }

    RGB{r: rgb_val[0], g: rgb_val[1], b: rgb_val[2]}
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let wifi_init =
        esp_wifi::init(timer1.timer0, rng).expect("Failed to initialize WIFI/BLE controller");
    let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(&wifi_init, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");


    let mut spibus = Spi::new(peripherals.SPI2, SpiConfig::default()
        .with_frequency(Rate::from_mhz(60))
        .with_mode(SpiMode::_0)
    )
        .unwrap()
        .with_sck(peripherals.GPIO39)
        .with_mosi(peripherals.GPIO40);



    let _ = spawner;

    loop {
        let rgb = color_shifter();
        let words: [u8; 12] = [
            0x00, 0x00, 0x00, 0x00,     // start frame
            0xE8, rgb.b, rgb.g, rgb.r,  // E8 = 1110_0000 + 1_0000 brightness 8, 0xFF seemed to work too.. data sheet shows BGR order weirdly
            0xFF, 0xFF, 0xFF, 0xFF,     // end frame/latch
        ];
        spibus.write(&words).unwrap();
        // info!("Hello world!");
        Timer::after(Duration::from_millis(50)).await;
    }
}

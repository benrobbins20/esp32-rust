#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer, Instant};
use esp_hal::gpio::{Level, OutputConfig};
use esp_hal::{clock::CpuClock, gpio::Output};
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_alloc::HEAP;
use log::{info, debug};
use esp_println::println;
extern crate alloc;

// no_std requires a panic handler, default to non-divergent (!) infinite loop 
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// setup heap manually
// macro equivalent = esp_alloc::heap_allocator!(size: 72 * 1024)
/* 
#[global_allocator]
static ALLOCATOR: EspHeap = esp_alloc::EspHeap::empty(); // like new(), our own instance of HEAP 

fn init_heap() {

    // 72 KB of internal RAM, s3 has 512 KB
    const SIZE: usize = 72 * 1024; 

    // array[type: size]
    // so a buffer of 72 KB of single bytes, init to 0x0
       this is the OLD way, now HEAP is of type MaybeUninit which doesn't write 0s to buffer yet until 
    static mut BUFFER: [u8; SIZE] = [0; SIZE]; 

    // has to be wrapped in unsafe because its a mutable static/global
    // point to the first byte (*u8) of buffer, set size
    // capabilities meaning, 
    unsafe {
        ALLOCATOR.add_region(HeapRegion::new(
            BUFFER.as_mut_ptr() as *mut u8,
            SIZE,
            MemoryCapability::Internal.into()
            ));
    }
}
*/

// create a blinky task
#[embassy_executor::task]
async fn blinky_task(led: Output<'static>) {
    let mut led = led;
    loop {
        info!("Blinky - info");
        led.toggle();
        Timer::after(Duration::from_millis(500)).await;
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


    // easy macro, manual method above
    // no attributes eg #[link_section = ".dram2_uninit"]
    // so just passing size: $size:expr to HEAP 
    // static mut HEAP: core::mem::MaybeUninit<[u8; $size]> = core::mem::MaybeUninit::uninit(); // buffer of size 
    // MaybeUninit<[u8; $size]> uninitialized array of size bytes
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
    let wifi_init = esp_wifi::init(timer1.timer0, rng)
        .expect("Failed to initialize WIFI/BLE controller");
    let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(&wifi_init, peripherals.WIFI)
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
    debug!("wifi started: {:?}", _wifi_controller.is_started());
    debug!("wifi capabilities: {:?}", _wifi_controller.capabilities());
    let mac = _interfaces.sta.mac_address();
    debug!("sta mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]); 

    // led for blinky gpio21 
    let led: Output<'_> = esp_hal::gpio::Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());

    // TODO: Spawn some tasks
    let _ = spawner;

    spawner.spawn(blinky_task(led)).unwrap();

    // the tasks have a non-blocking loop
    // loop {
    //     Timer::after(Duration::from_secs(1)).await;
    // }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
    
}

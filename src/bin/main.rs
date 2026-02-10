#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::sync::atomic::{AtomicI32};
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_futures::select::select;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Io, Level, Output, OutputConfig, Pull};
use esp_hal::rtc_cntl::sleep::GPIO_INTR_HIGH_LEVEL;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_println::print;
use log::info;

use esp_hal::handler;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// static delay for atomic read/write speed
static DELAY: AtomicI32 = AtomicI32::new(0);

#[embassy_executor::task]
async fn blinky_task(led: Output<'static>) {
    let mut led = led;
    loop {
        info!("Blinky - info");
        led.toggle();
        Timer::after(Duration::from_millis(500)).await;
    }   
}

#[embassy_executor::task]
async fn motor(step: Output<'static>, dir: Output<'static>) {
    let mut step = step;
    let mut dir = dir;
    loop {
        // relaxed: no ordering before or after load
        // acquire: store.release -> load.acquire
        let delay = DELAY.load(Ordering::Relaxed);

        // do nothing if zero or default
        if (delay == 0) {
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }
        
        // set direction
        if (delay > 0) {
            dir.set_high();
        }
        else {
            dir.set_low();
        }

        // set limits for delay
        let delay = delay.abs()
            .max(50) // this is the LOWER limit
            .min(100_000); // upper limit 


        step.toggle();
        Timer::after(Duration::from_micros(delay as u64)).await;
    }
}

#[embassy_executor::task]
async fn encoder(a: Input<'static>, b: Input<'static>) {

    let mut rotary_encoder = rotary_encoder_hal::Rotary::new(a, b);
    let mut counter = 0i32;

    loop {
        // borrow pins from the encoder owned pins
        let (a,b) = rotary_encoder.pins();
        let result = select(a.wait_for_any_edge(),b.wait_for_any_edge()).await;
        let direction = rotary_encoder.update().unwrap();
        match direction {
            rotary_encoder_hal::Direction::Clockwise => counter += 1,
            rotary_encoder_hal::Direction::CounterClockwise => counter -= 1,
            rotary_encoder_hal::Direction::None => (),
        }
        print!("Counter: {}\n", counter);

        // microseconds / delay.pow(3)
        // ex: 1_000_000 / 100^3 = 1 second
        if counter != 0 {
            DELAY.store(1_000_000 / counter.pow(3), Ordering::Relaxed);
        }

        // no delay or no speed?
        else {
            DELAY.store(0, Ordering::Relaxed);
        }

        DELAY.store(counter, Ordering::Relaxed);
    }
}



#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // let mut io = Io::new(peripherals.IO_MUX);
    esp_hal::interrupt::enable(esp_hal::peripherals::Interrupt::GPIO, esp_hal::interrupt::Priority::Priority1).unwrap(); 
    
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");
    Timer::after(Duration::from_millis(200)).await; // delay is required with for the info! macro in embassy task

    // let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    // let timer1 = TimerGroup::new(peripherals.TIMG0);
    // let wifi_init =
    //     esp_wifi::init(timer1.timer0, rng).expect("Failed to initialize WIFI/BLE controller");
    // let (mut _wifi_controller, _interfaces) = esp_wifi::wifi::new(&wifi_init, peripherals.WIFI)
    //     .expect("Failed to initialize WIFI controller");


    // assign pins
    let led: Output<'_> = Output::new(peripherals.GPIO7, Level::Low, OutputConfig::default()); 
    let a = Input::new(peripherals.GPIO0, InputConfig::default().with_pull(Pull::Up));
    let b = Input::new(peripherals.GPIO1, InputConfig::default().with_pull(Pull::Up));
    let step = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let dir = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());



  
    // TODO: Spawn some tasks
    let _ = spawner;
    spawner.spawn(blinky_task(led)).unwrap();
    spawner.spawn(encoder(a, b)).unwrap();
    spawner.spawn(motor(step, dir)).unwrap();
    // loop {
    //     info!("Hello world!");
    //     Timer::after(Duration::from_secs(1)).await;
    // }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}

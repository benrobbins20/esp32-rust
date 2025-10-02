#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![feature(impl_trait_in_assoc_type)]

use core::str::from_utf8;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use embassy_executor::Spawner;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::dns::DnsSocket;
use embassy_net::{new, Config, DhcpConfig, Runner, Stack, StackResources};
use embassy_time::{Delay, Duration, Instant, Timer};
use esp_hal::gpio::{Level, OutputConfig};
use esp_hal::system::AppCoreGuard;
use esp_hal::{clock::CpuClock, gpio::Output};
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_alloc::HEAP;
use esp_wifi::wifi::{ClientConfiguration, WifiController, WifiDevice, WifiState};
use esp_wifi::EspWifiController;
use log::{info, debug, error};
use esp_println::println;
use esp_hal::rng::Rng;
use picoserve::response::{IntoResponse, Response, StatusCode};
use picoserve::routing::{get, PathRouter};
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::{request, response};
use picoserve::io::Socket;
use picoserve::extract::State;

// use static_cell::{make_static, StaticCell};
extern crate alloc;
use esp_wifi::wifi::WifiEvent;
use esp_wifi::wifi::Configuration as WifiConfiguration;
use esp_wifi::wifi::AccessPointInfo;
use reqwless::request::Method::GET;
// use embedded_io_async::Read;
use static_cell::{make_static, StaticCell};

use picoserve::time::Timer as PicoTimer;
use picoserve::{make_static as make_static_pico, AppWithStateBuilder, Router};
use picoserve::{AppBuilder, AppRouter};
use core::sync::atomic::AtomicUsize;
use alloc::string::String;




const SSID: &str = env!("RUST_ESP32_STD_DEMO_WIFI_SSID");
const PASS: &str = env!("RUST_ESP32_STD_DEMO_WIFI_PASS");

// const URL: &str = "http://192.168.8.210:8000/";
// www.mobile-j.de

const HTTP_URL: &str = "http://192.168.8.210:8000/";
const HTTPS_URL: &str = "https://example.com/";

// all of these are being weird
// const HTTPS_URL: &str = "https://httpbin.org/get";
// const HTTPS_URL: &str = "https://resttesttest.com/";
// const HTTPS_URL: &str = "https://www.howsmyssl.com/";
// const HTTPS_URL: &str = "https://google.com/";

// make wifi_init RESULT static EspWifiController<'d>
// static WIFI_CONTROLLER: StaticCell<esp_wifi::EspWifiController<'static>> = StaticCell::new();

// static stack
// static STACK: StaticCell<embassy_net::Stack<'static>> = StaticCell::new();

// no_std requires a panic handler, default to non-divergent (!) infinite loop 
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// struct EmbassyTimer;

// impl picoserve::Timer for EmbassyTimer {
//     type Duration = embassy_time::Duration;
//     type TimeoutError = embassy_time::TimeoutError;

//     async fn run_with_timeout<F: core::future::Future>(
//             &mut self,
//             duration: Self::Duration,
//             future: F,
//         ) -> Result<F::Output, Self::TimeoutError> {
//             embassy_time::with_timeout(duration, future).await
//         }
// }

// TaskRouter holds a message str to pass between 
// struct TaskRouter {
//     task_id: usize,
// }

// impl TaskRouter {
//     fn new(task_id: usize) -> Self {
//         Self { task_id}
//     }
// }

// impl AppBuilder for TaskRouter {
//     type PathRouter = impl picoserve::routing::PathRouter;

//     fn build_app(self) -> Router<Self::PathRouter> {
//         let task_id = self.task_id;
//         // let content = format!("Hello from task {}!", task_id);
//         // let resp = Response::new(StatusCode::OK, content);
//         Router::new()
//             .route(
//                 "/", 
//                 get(move || async move {
//                     let content = format!("Hello from task {}!", task_id);
//                     Response::new(StatusCode::OK, content.as_str())
//             }),
//         )
//     }
// }

// // try with state method
// struct AppProps {
//     service_name: &'static str,
// }
// // properties of the app, which a builder that can construct the task router/unique ID's
// struct TaskState {
//     state_request_count: AtomicUsize,
// }

// impl AppWithStateBuilder for AppProps {
//     type State = TaskState;
//     type PathRouter = impl picoserve::routing::PathRouter<TaskState>;
    
//     fn build_app(self) -> Router<Self::PathRouter> {
//         let service_name = self.service_name;
//         Router::new()
//             .route(
//                 "/", 
//                 get(move |state: State<TaskState>| async move {
//                     let count = state.request_count.fetch_add(1, core::sync::atomic::Ordering::SeqCst) + 1;
//                     let content = format!("Hello from {}! This endpoint has been called {} times.", service_name, count);
//                     Response::new(StatusCode::OK, content.as_str())
//             }),
//         )
//     }
    
// }

#[derive(Clone)]
struct AppState {
    task_id: usize,
}

const WEB_TASK_POOL_SIZE: usize = 2;

// async fn handler() -> impl IntoResponse {
//     "hello from picoserve!"
// }

// async fn handler(state: State<AppState>) -> impl IntoResponse {
//     format!("Hello from task {}!", state.task_id).as_str()
// }

async fn handler(msg: &'static str) -> impl IntoResponse {
    msg
}

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
async fn web_task(
    task_id: usize,
    stack: &'static Stack<'static>,
    config: &'static picoserve::Config<Duration>,
    // app: &'static AppRouter<TaskRouter>
    msg: &'static str,
) -> ! {
    let port = 80;
    let mut tcp_rx_buffer = [0; 1024];
    let mut tcp_tx_buffer = [0; 1024];
    let mut http_buffer = [0; 2048];

    stack.wait_link_up().await;
    stack.wait_config_up().await;
    
    info!("Starting web-task");
    
    // wait for the stack to come up
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Network link is up");
    info!("Obtaining IP address...");

    // wait for ip address via dhcp
    loop {
        // Some(Option<StaticConfigV4>)
        if let Some(config) = stack.config_v4() {
            info!("IP address obtained: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // let state = AppState { task_id };


    // let app = Router::new()
    //     .route("/", get(handler))
    // ;
    info!("msg: {}", msg);
    let app = Router::new().route("/", get(move || async move {msg}));

    picoserve::listen_and_serve(
        task_id,
        &app,
        config,
        *stack,
        port,
        &mut tcp_rx_buffer,
        &mut tcp_tx_buffer,
        &mut http_buffer,

    )
    .await
    
}

// create a blinky task
#[embassy_executor::task]
async fn blinky_task(led: Output<'static>) {
    let mut led = led;
    loop {
        // info!("Blinky - info");
        led.toggle();
        Timer::after(Duration::from_millis(500)).await;
    }   
}

// async task for wifi connection
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    info!("starting connection task");
    info!("Device capabilities: {:?}", controller.capabilities());
    
    // main connection loop, split into various paths as loop runs
    loop {
        match esp_wifi::wifi::sta_state() {
           WifiState::StaConnected => {
                // info!("Connected to wifi!");
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                error!("Disconnected from wifi");
                Timer::after(Duration::from_millis(5_000)).await;
           }
           _ => {
                error!("Not connected to wifi. Retrying...");
                Timer::after(Duration::from_millis(1_000)).await;
           } 
        }
        
        // check if controller is started
        // if controller.is_started() != Ok(true), meaning its not started, start it
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = WifiConfiguration::Client(ClientConfiguration {
                ssid: SSID.into(),
                password: PASS.into(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            info!("Starting wifi...");
            controller.start().unwrap();
            info!("wifi started");
        
            // scan for ap's
            info!("Scanning for access points...");
            let scan_config = esp_wifi::wifi::ScanConfig::default();
            let result: Vec<AccessPointInfo> = controller
                .scan_with_config_async(scan_config)
                .await
                .unwrap();
            info!("found {} access points", result.len());
            for ap in result {
                info!("{:?}", ap); // can i actually just print the struct?
            }

            // final connection branch
        }
        info!("Connecting to AP: {}", SSID);
        match controller.connect_async().await {
            Ok(_) => info!("Connected to AP: {}", SSID),
            Err(e) => {
                error!("Failed to connect to AP: {}, error: {:?}", SSID, e);
                Timer::after(Duration::from_millis(5000)).await;
            }
        }
    }
}

// run the network stack
#[embassy_executor::task]
async fn stack_runner(mut runner: Runner<'static, WifiDevice<'static>>) {
    info!("Starting network stack runner");
    runner.run().await;
}

// check IP related stuff
#[embassy_executor::task]
async fn ip_task(stack: &'static Stack<'static>, tls_seed: u64) {
    info!("Starting IP task");
    
    // wait for the stack to come up
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    info!("Network link is up");
    info!("Obtaining IP address...");

    // wait for ip address via dhcp
    loop {
        // Some(Option<StaticConfigV4>)
        if let Some(config) = stack.config_v4() {
            info!("IP address obtained: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    // create a embassy tcp client and send http with reqwless
    
    // tcp buffers
    let mut rx_buf = [0; 10000];
    let mut tx_buf = [0; 10000];

    // tls buffers
    let mut tls_read_buffer = [0; 4096];
    let mut tls_write_buffer = [0; 4096];

    // set up the embassy-net client to pass to reqwless
    let client_state = TcpClientState::<4, 4096, 4096>::new();
    let client = TcpClient::new(stack.clone(), &client_state);
    let dns = DnsSocket::new(stack.clone());

    // TLS setup - requires the root CA certificate of the server
    let tls = TlsConfig::new(tls_seed, &mut tls_read_buffer, &mut tls_write_buffer, TlsVerify::None);
    let mut https_client = HttpClient::new_with_tls(&client, &dns, tls);

    loop {
        // create and send request with reqwless
        // let mut http_client = HttpClient::new(&client, &dns);

        // create https client

        // unwrapping was causing panics that couldnt be seen and it hung the scheduler
        // let mut request = http_client.request(reqwless::request::Method::GET, "http://192.168.8.210:8000/").await.unwrap();

        // handle bad request
        // let mut req = match http_client.request(GET, HTTP_URL).await {
        //     Ok(req) => req,
        //     Err(e) => {
        //         error!("Failed to create request: {:?}", e);
        //         Timer::after(Duration::from_millis(500)).await;
        //         continue; // skip to next loop iteration
        //     }
        // };

        let mut req = https_client.request(GET, HTTPS_URL).await.unwrap();
        
        let resp = req.send(&mut rx_buf).await.unwrap();


        info!("{:?}",resp.status);
        // let body = response.body(); its not this simple?
        //let body = from_utf8(resp.body().read_to_end().await.unwrap());

        let body_bytes = resp.body().read_to_end().await.unwrap();
        let body = from_utf8(&body_bytes).unwrap();


        
        // let body_read = embedded_io::Read::read(&rx_buf)

        // getting chunked data, try with embedded_io_async::Read;
        // let mut body = resp.body().Read
        info!("body: {:?}", body);

        Timer::after(Duration::from_millis(1000)).await;
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
    let hw_config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    debug!("{:?}", hw_config.cpu_clock());
    let peripherals = esp_hal::init(hw_config); // can debug peripherals pretty minimally after they are assigned


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
    // timer for embassy scheduler
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    // configure wifi peripheral with 
    let mut rng = Rng::new(peripherals.RNG);
    let nw_stack_seed = (rng.random() as u64) << 32 | rng.random() as u64; // card shuffle random seed
    let tls_seed = (rng.random() as u64) << 32 | rng.random() as u64; // tls random seed
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    // this init() was returning EspWifiController<'d> 
    // 'd (rng, timg0) which live only as long 
    let wifi_init = esp_wifi::init(timg0.timer0, rng).expect("WIFI/BLE controller");
    // let wifi_init = make_static!(EspWifiController<'static>, esp_wifi::init(timg0.timer0, rng));
    let wifi_ctrl = make_static_pico!(EspWifiController<'static>, wifi_init);

    let (wifi_controller, interfaces) = esp_wifi::wifi::new(wifi_ctrl, peripherals.WIFI)
        .expect("Failed to initialize WIFI controller");
    
    // pull out a station client
    let station = interfaces.sta;

    /*                  DEBUG                    */ 
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
    let mac = station.mac_address();
    debug!("station mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]); 



    // configure network stack
    let nw_config = embassy_net::Config::dhcpv4(DhcpConfig::default());
    let (stack_,runner) = embassy_net::new(
        station,
        nw_config,
        make_static_pico!(StackResources::<10>, StackResources::<10>::new()),
        nw_stack_seed
    );

    
    let stack = make_static_pico!(Stack<'static>, stack_);


    // led for blinky gpio21 
    let led: Output<'_> = esp_hal::gpio::Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());

    let config = make_static_pico!(
        picoserve::Config<Duration>,
        picoserve::Config::new(picoserve::Timeouts {
            start_read_request: Some(Duration::from_secs(5)),
            persistent_start_read_request: Some(Duration::from_secs(1)),
            read_request: Some(Duration::from_secs(1)),
            write: Some(Duration::from_secs(1)),
        })
        .keep_connection_alive()
    );

    

    // TODO: Spawn some tasks
    let _ = spawner;
    spawner.must_spawn(stack_runner(runner));

    spawner.spawn(blinky_task(led)).unwrap();
    spawner.spawn(connection(wifi_controller)).unwrap();
    
    // // spawner.spawn(ip_task(stack, tls_seed)).unwrap();

    // let app = make_static_pico!(AppRouter<TaskRouter>, TaskRouter::new(0).build_app());

    // for id in 0..WEB_TASK_POOL_SIZE {
    //     let s = make_static_pico!(String, format!("Hello from task id: {}", id));
    //     let msg: &'static str = s.as_str();

    //     spawner.must_spawn(web_task(id, stack, config, msg));
    // }
    let string1 = make_static_pico!(String, String::from("hello from task 0"));
    let string2 = make_static_pico!(String, String::from("hello from task 1"));
    let msg1: &'static str = string1.as_str();
    let msg2: &'static str = string2.as_str();
    spawner.must_spawn(web_task(0, stack, config, msg1));
    spawner.must_spawn(web_task(1, stack, config, msg2));

    // the tasks have a non-blocking loop
    // loop {
    //     Timer::after(Duration::from_secs(1)).await;
    // }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
    
}

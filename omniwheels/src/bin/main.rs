#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::esp_now::BROADCAST_ADDRESS;
use esp_wifi::wifi::{Configuration, ClientConfiguration};
use common::MessageType;
use static_cell::StaticCell;
use esp_wifi::EspWifiController;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {:?}", info);
    loop {}
}

extern crate alloc;

static WIFI_INIT: StaticCell<EspWifiController<'static>> = StaticCell::new();

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    // Initialize WiFi for ESP-NOW
    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let init = esp_wifi::init(
        timer1.timer0,
        esp_hal::rng::Rng::new(peripherals.RNG),
    )
    .unwrap();
    let init = WIFI_INIT.init(init);

    // Create WiFi interfaces - this gives us ESP-NOW
    let (mut wifi_controller, interfaces) = esp_wifi::wifi::new(init, peripherals.WIFI).unwrap();
    let mut esp_now = interfaces.esp_now;

    // Start WiFi in STA mode (required for ESP-NOW)
    wifi_controller.set_configuration(&Configuration::Client(ClientConfiguration::default())).unwrap();
    wifi_controller.start().unwrap();
    println!("WiFi started in STA mode");

    println!("ESP-NOW initialized, version: {:?}", esp_now.version());
    println!("Omniwheels minimal ESP-NOW PoC started - waiting for pings...");

    loop {
        // Check for incoming messages
        if let Some(received) = esp_now.receive() {
            let src = received.info.src_address;
            println!("Received from {:?}: {:?}", src, received.data());

            if let Some(MessageType::Ping) = received.data().first().and_then(|&b| MessageType::from_byte(b)) {
                println!("Received ping, sending pong...");
                let pong_data = [MessageType::Pong as u8];
                match esp_now.send(&BROADCAST_ADDRESS, &pong_data) {
                    Ok(_) => println!("Sent pong"),
                    Err(e) => println!("Send error: {:?}", e),
                }
            }
        }
        Timer::after(Duration::from_millis(100)).await;
    }
}

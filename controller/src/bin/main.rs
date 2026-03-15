#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    timer::systimer::SystemTimer,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_wifi::esp_now::{EspNow, BROADCAST_ADDRESS};
use esp_wifi::EspWifiController;
use common::MessageType;
use static_cell::StaticCell;

extern crate alloc;

static WIFI_INIT: StaticCell<EspWifiController<'static>> = StaticCell::new();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {:?}", info);
    loop {}
}

// Simple LED on a GPIO - for bare minimum, we use a regular LED instead of WS2812
// If you have a regular LED on GPIO8 or another pin, this will work
// For WS2812 RGB LED, we'd need the RMT peripheral with precise timing

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
        peripherals.RADIO_CLK,
    )
    .unwrap();
    let init = WIFI_INIT.init(init);

    // Use GPIO8 as simple output (for onboard LED or external LED)
    // Note: The ESP32-C6 dev board's onboard LED is WS2812, but we can
    // still toggle GPIO8 - it just won't show colors properly
    let mut led = Output::new(peripherals.GPIO8, Level::High, OutputConfig::default());
    println!("Controller starting - LED on (searching)");

    // Initialize ESP-NOW
    let mut esp_now = EspNow::new(init, peripherals.WIFI).unwrap();
    println!("ESP-NOW initialized, version: {:?}", esp_now.version());

    let mut connected = false;
    let mut blink_state = false;

    loop {
        // Blink while not connected
        if !connected {
            blink_state = !blink_state;
            if blink_state {
                led.set_high();
            } else {
                led.set_low();
            }

            // Send ping broadcast
            let ping_data = [MessageType::Ping as u8];
            match esp_now.send(&BROADCAST_ADDRESS, &ping_data) {
                Ok(_) => println!("Sent ping"),
                Err(e) => println!("Send error: {:?}", e),
            }
        }

        // Check for incoming messages
        if let Some(received) = esp_now.receive() {
            println!("Received from {:?}: {:?}", received.info.src_address, received.data());
            if let Some(MessageType::Pong) = received.data().first().and_then(|&b| MessageType::from_byte(b)) {
                if !connected {
                    println!("Received pong - connected!");
                    connected = true;
                    led.set_high(); // Solid on when connected
                }
            }
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

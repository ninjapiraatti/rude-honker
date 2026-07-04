#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::{
    analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation},
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    timer::systimer::SystemTimer,
    timer::timg::TimerGroup,
};
use esp_println::println;
use esp_wifi::esp_now::BROADCAST_ADDRESS;
use esp_wifi::wifi::{Configuration, ClientConfiguration};
use esp_wifi::EspWifiController;
use common::{DriveMode, MessageType, MoveCommand};
use static_cell::StaticCell;

extern crate alloc;

static WIFI_INIT: StaticCell<EspWifiController<'static>> = StaticCell::new();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {:?}", info);
    loop {}
}

// Joystick deadzone (deviation from center, in calibrated millivolts)
const DEADZONE: i16 = 150;

/// Convert a calibrated ADC reading (millivolts) to a joystick value (-100..100).
///
/// With ADC calibration the reading is linear in mV. A joystick pot is
/// ratiometric, so it rests at ~Vsupply/2 and swings 0..Vsupply — meaning the
/// span in each direction is simply `center`.
fn adc_to_joystick(mv: u16, center: u16) -> i16 {
    let centered = (mv as i32) - (center as i32);
    if centered.unsigned_abs() < DEADZONE as u32 {
        return 0;
    }
    ((centered * 100) / center as i32).clamp(-100, 100) as i16
}

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

    // Use GPIO8 as simple output for status indication
    let mut led = Output::new(peripherals.GPIO8, Level::High, OutputConfig::default());
    println!("Controller starting - LED on (searching)");

    // Setup ADC for joystick (GPIO3 = VRx, GPIO4 = VRy)
    let mut adc_config = AdcConfig::new();
    let mut vrx_pin = adc_config.enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO3, Attenuation::_11dB);
    let mut vry_pin = adc_config.enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    // Joystick button (GPIO5 with internal pull-up, active low) - toggles drive mode
    let button = Input::new(peripherals.GPIO5, InputConfig::default().with_pull(Pull::Up));

    // Calibrate joystick center - sample multiple times and average
    println!("Calibrating joystick - keep it centered...");
    let mut vrx_sum: u32 = 0;
    let mut vry_sum: u32 = 0;
    const CAL_SAMPLES: u32 = 16;
    for _ in 0..CAL_SAMPLES {
        vrx_sum += nb::block!(adc.read_oneshot(&mut vrx_pin)).unwrap_or(2048) as u32;
        vry_sum += nb::block!(adc.read_oneshot(&mut vry_pin)).unwrap_or(2048) as u32;
    }
    let vrx_center = (vrx_sum / CAL_SAMPLES) as u16;
    let vry_center = (vry_sum / CAL_SAMPLES) as u16;
    println!("Joystick calibrated: center X={}, Y={}", vrx_center, vry_center);

    // Create WiFi interfaces - this gives us ESP-NOW
    let (mut wifi_controller, interfaces) = esp_wifi::wifi::new(init, peripherals.WIFI).unwrap();
    let mut esp_now = interfaces.esp_now;

    // Start WiFi in STA mode (required for ESP-NOW)
    wifi_controller.set_configuration(&Configuration::Client(ClientConfiguration::default())).unwrap();
    wifi_controller.start().unwrap();
    println!("WiFi started in STA mode");

    println!("ESP-NOW initialized, version: {:?}", esp_now.version());

    let mut connected = false;
    let mut blink_state = false;
    let mut last_move = MoveCommand::default();
    let mut mode = DriveMode::Strafe;
    let mut button_was_down = false;

    loop {
        // Toggle drive mode on button press (falling edge, active low)
        let button_down = button.is_low();
        if button_down && !button_was_down {
            mode = match mode {
                DriveMode::Strafe => DriveMode::Rotate,
                DriveMode::Rotate => DriveMode::Strafe,
            };
            println!("Drive mode: {:?}", mode);
        }
        button_was_down = button_down;

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
        } else {
            // Read joystick and send movement commands
            let vrx_raw: u16 = nb::block!(adc.read_oneshot(&mut vrx_pin)).unwrap_or(vrx_center);
            let vry_raw: u16 = nb::block!(adc.read_oneshot(&mut vry_pin)).unwrap_or(vry_center);

            let x = adc_to_joystick(vrx_raw, vrx_center);
            let y = adc_to_joystick(vry_raw, vry_center);

            // Only send if changed significantly, or the mode changed
            if (x - last_move.x).abs() > 5 || (y - last_move.y).abs() > 5 || mode != last_move.mode {
                let cmd = MoveCommand { x, y, mode };
                match esp_now.send(&BROADCAST_ADDRESS, &cmd.to_bytes()) {
                    Ok(_) => {
                        if x != 0 || y != 0 {
                            println!("Move: x={}, y={} ({:?})", x, y, mode);
                        }
                    }
                    Err(e) => println!("Send error: {:?}", e),
                }
                last_move = cmd;
            }
        }

        // Check for incoming messages
        if let Some(received) = esp_now.receive() {
            if let Some(MessageType::Pong) = received.data().first().and_then(|&b| MessageType::from_byte(b)) {
                if !connected {
                    println!("Received pong - connected!");
                    connected = true;
                    led.set_high(); // Solid on when connected
                }
            }
        }

        Timer::after(Duration::from_millis(50)).await;
    }
}

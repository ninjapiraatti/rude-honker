#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::mcpwm::{operator::PwmPinConfig, timer::PwmWorkingMode, McPwm, PeripheralClockConfig};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::esp_now::BROADCAST_ADDRESS;
use esp_wifi::wifi::{Configuration, ClientConfiguration};
use common::{DriveMode, MessageType, MoveCommand};
use static_cell::StaticCell;
use esp_wifi::EspWifiController;
use tb6612fng::{DriveCommand, Motor};

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {:?}", info);
    loop {}
}

extern crate alloc;

static WIFI_INIT: StaticCell<EspWifiController<'static>> = StaticCell::new();

/// Convert joystick x,y (-100..100) to individual motor speeds for omniwheel drive
/// Returns (front_left, front_right, back_left, back_right)
/// Each value is -100..100 where positive = forward
fn omniwheel_mix(x: i16, y: i16, mode: DriveMode) -> (i8, i8, i8, i8) {
    // y always drives forward/back (all wheels together). The x term differs:
    let (fl, fr, bl, br) = match mode {
        // Strafe: FL & BR pair against FR & BL (mecanum sideways translation).
        DriveMode::Strafe => (y + x, y - x, y - x, y + x),
        // Rotate: left wheels against right wheels (spin in place).
        DriveMode::Rotate => (y + x, y - x, y + x, y - x),
    };
    (
        fl.clamp(-100, 100) as i8,
        fr.clamp(-100, 100) as i8,
        bl.clamp(-100, 100) as i8,
        br.clamp(-100, 100) as i8,
    )
}

/// Convert signed speed (-100..100) to DriveCommand
fn speed_to_command(speed: i8) -> DriveCommand {
    if speed > 5 {
        DriveCommand::Forward(speed as u8)
    } else if speed < -5 {
        DriveCommand::Backward((-speed) as u8)
    } else {
        DriveCommand::Stop
    }
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

    // ========== MOTOR SETUP ==========
    // Motor direction pins
    let motor_a1_pin1 = Output::new(peripherals.GPIO23, Level::Low, OutputConfig::default());
    let motor_a1_pin2 = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    let motor_a2_pin1 = Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());
    let motor_a2_pin2 = Output::new(peripherals.GPIO20, Level::Low, OutputConfig::default());
    let motor_b1_pin1 = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
    let motor_b1_pin2 = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());
    let motor_b2_pin1 = Output::new(peripherals.GPIO9, Level::Low, OutputConfig::default());
    let motor_b2_pin2 = Output::new(peripherals.GPIO13, Level::Low, OutputConfig::default());
    let mut stdby_pin = Output::new(peripherals.GPIO11, Level::Low, OutputConfig::default());

    // PWM pins
    let motor_a1_pwm_pin = peripherals.GPIO15;
    let motor_a2_pwm_pin = peripherals.GPIO19;
    let motor_b1_pwm_pin = peripherals.GPIO2;
    let motor_b2_pwm_pin = peripherals.GPIO12;

    // MCPWM setup
    let clock_cfg = PeripheralClockConfig::with_frequency(Rate::from_mhz(40)).unwrap();
    let mut mcpwm_a = McPwm::new(peripherals.MCPWM0, clock_cfg);
    mcpwm_a.operator0.set_timer(&mcpwm_a.timer0);
    mcpwm_a.operator1.set_timer(&mcpwm_a.timer1);

    let motor_a1_pwm = mcpwm_a.operator0.with_pins(
        motor_a1_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH,
        motor_a2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH,
    );
    let motor_b1_pwm = mcpwm_a.operator1.with_pins(
        motor_b1_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH,
        motor_b2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH,
    );

    let mut motor_a1 = Motor::new(motor_a1_pin1, motor_a1_pin2, motor_a1_pwm.0).unwrap();
    let mut motor_a2 = Motor::new(motor_a2_pin1, motor_a2_pin2, motor_a1_pwm.1).unwrap();
    let mut motor_b1 = Motor::new(motor_b1_pin1, motor_b1_pin2, motor_b1_pwm.0).unwrap();
    let mut motor_b2 = Motor::new(motor_b2_pin1, motor_b2_pin2, motor_b1_pwm.1).unwrap();

    let timer_clock_cfg = clock_cfg
        .timer_clock_with_frequency(99, PwmWorkingMode::Increase, Rate::from_khz(20))
        .unwrap();
    mcpwm_a.timer0.start(timer_clock_cfg);
    mcpwm_a.timer1.start(timer_clock_cfg);

    // Enable motor driver
    stdby_pin.set_high();
    println!("Motors initialized");

    // Create WiFi interfaces - this gives us ESP-NOW
    let (mut wifi_controller, interfaces) = esp_wifi::wifi::new(init, peripherals.WIFI).unwrap();
    let mut esp_now = interfaces.esp_now;

    // Start WiFi in STA mode (required for ESP-NOW)
    wifi_controller.set_configuration(&Configuration::Client(ClientConfiguration::default())).unwrap();
    wifi_controller.start().unwrap();
    println!("WiFi started in STA mode");

    println!("ESP-NOW initialized, version: {:?}", esp_now.version());
    println!("Omniwheels ready - waiting for controller...");

    loop {
        // Check for incoming messages
        if let Some(received) = esp_now.receive() {
            let data = received.data();

            match data.first().and_then(|&b| MessageType::from_byte(b)) {
                Some(MessageType::Ping) => {
                    let pong_data = [MessageType::Pong as u8];
                    let _ = esp_now.send(&BROADCAST_ADDRESS, &pong_data);
                    println!("Pong sent");
                }
                Some(MessageType::Move) => {
                    if let Some(cmd) = MoveCommand::from_bytes(data) {
                        let (fl, fr, bl, br) = omniwheel_mix(cmd.x, cmd.y, cmd.mode);

                        motor_a1.drive(speed_to_command(fl)).ok();
                        motor_a2.drive(speed_to_command(fr)).ok();
                        motor_b1.drive(speed_to_command(bl)).ok();
                        motor_b2.drive(speed_to_command(br)).ok();

                        if cmd.x != 0 || cmd.y != 0 {
                            println!("Move x={} y={} ({:?}) -> FL:{} FR:{} BL:{} BR:{}",
                                cmd.x, cmd.y, cmd.mode, fl, fr, bl, br);
                        }
                    }
                }
                _ => {}
            }
        }
        Timer::after(Duration::from_millis(10)).await;
    }
}

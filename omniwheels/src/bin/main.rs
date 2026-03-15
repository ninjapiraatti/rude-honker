#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    mcpwm::{operator::PwmPinConfig, timer::PwmWorkingMode, McPwm, PeripheralClockConfig},
    time::Rate
};
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::esp_now::{EspNow, BROADCAST_ADDRESS};
use common::MessageType;
use static_cell::StaticCell;
use esp_wifi::EspWifiController;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {:?}", info);
    loop {}
}
use tb6612fng::{DriveCommand, Motor};

extern crate alloc;

static WIFI_INIT: StaticCell<EspWifiController<'static>> = StaticCell::new();
static ESP_NOW: StaticCell<EspNow<'static>> = StaticCell::new();

#[embassy_executor::task]
async fn esp_now_task(esp_now: &'static mut EspNow<'static>) {
    println!("ESP-NOW task started, waiting for messages...");

    loop {
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
        Timer::after(Duration::from_millis(10)).await;
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.3.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let init = esp_wifi::init(
        timer1.timer0,
        esp_hal::rng::Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
    )
    .unwrap();
    let init = WIFI_INIT.init(init);

    // Initialize ESP-NOW
    let esp_now = EspNow::new(init, peripherals.WIFI).unwrap();
    println!("ESP-NOW initialized, version: {:?}", esp_now.version());
    let esp_now = ESP_NOW.init(esp_now);
    spawner.spawn(esp_now_task(esp_now)).unwrap();

    // Motors
    let motor_a1_pin1 = Output::new(peripherals.GPIO23, Level::Low, OutputConfig::default());
    let motor_a1_pin2 = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    let motor_a2_pin1 = Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());
    let motor_a2_pin2 = Output::new(peripherals.GPIO20, Level::Low, OutputConfig::default());
    let motor_b1_pin1 = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
    let motor_b1_pin2 = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());
    let motor_b2_pin1 = Output::new(peripherals.GPIO9, Level::Low, OutputConfig::default());
    let motor_b2_pin2 = Output::new(peripherals.GPIO13, Level::Low, OutputConfig::default());
    let mut stdby_pin = Output::new(peripherals.GPIO11, Level::Low, OutputConfig::default());

    let motor_a1_pwm_pin = peripherals.GPIO15;
    let motor_a2_pwm_pin = peripherals.GPIO19;
    let motor_b1_pwm_pin = peripherals.GPIO2;
    let motor_b2_pwm_pin = peripherals.GPIO12;

    let clock_cfg = PeripheralClockConfig::with_frequency(Rate::from_mhz(40)).unwrap();
    let mut mcpwm_a = McPwm::new(peripherals.MCPWM0, clock_cfg);
    //let mut mcpwm_b = McPwm::new(peripherals.MCPWM1, clock_cfg);
    mcpwm_a.operator0.set_timer(&mcpwm_a.timer0);
    mcpwm_a.operator1.set_timer(&mcpwm_a.timer1);
    //mcpwm_b.operator0.set_timer(&mcpwm_b.timer0);
    //mcpwm_b.operator1.set_timer(&mcpwm_b.timer1);
    let motor_a1_pwm = mcpwm_a.operator0.with_pins(motor_a1_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH, motor_a2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    //let motor_a2_pwm = mcpwm_a.operator0.with_pin_b(motor_a2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    let motor_b1_pwm = mcpwm_a.operator1.with_pins(motor_b1_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH, motor_b2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    //let motor_b2_pwm = mcpwm_a.operator1.with_pin_b(motor_b2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    let mut motor_a1 = Motor::new(motor_a1_pin1, motor_a1_pin2, motor_a1_pwm.0).unwrap();
    let mut motor_a2 = Motor::new(motor_a2_pin1, motor_a2_pin2, motor_a1_pwm.1).unwrap();
    let mut motor_b1 = Motor::new(motor_b1_pin1, motor_b1_pin2, motor_b1_pwm.0).unwrap();
    let mut motor_b2 = Motor::new(motor_b2_pin1, motor_b2_pin2, motor_b1_pwm.1).unwrap();

    let timer_clock_cfg = clock_cfg
        .timer_clock_with_frequency(99, PwmWorkingMode::Increase, Rate::from_khz(20))
        .unwrap();
    mcpwm_a.timer0.start(timer_clock_cfg);
    mcpwm_a.timer1.start(timer_clock_cfg);
    //mcpwm_b.timer0.start(timer_clock_cfg);
    //mcpwm_b.timer1.start(timer_clock_cfg);
    stdby_pin.set_high();

    println!("Omniwheels started, ESP-NOW running in background");

    loop {
        //Timer::after(Duration::from_secs(1)).await;
        let delay = Delay::new();
        let interval = 1000;
        let speed = 100;

        motor_a1.drive(DriveCommand::Stop).expect("driving");
        motor_a2.drive(DriveCommand::Stop).expect("driving");
        motor_b1.drive(DriveCommand::Stop).expect("driving");
        motor_b2.drive(DriveCommand::Stop).expect("driving");
        delay.delay_millis(interval);

        motor_a1.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("A1: {:?}", motor_a1.current_drive_command());
        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Backward(speed)).expect("driving");
        println!("A1: {:?}", motor_a1.current_drive_command());
        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Stop).expect("driving");

        motor_a2.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("A2: {:?}", motor_a2.current_drive_command());
        delay.delay_millis(interval);
        motor_a2.drive(DriveCommand::Backward(speed)).expect("driving");
        println!("A2: {:?}", motor_a2.current_drive_command());
        delay.delay_millis(interval);
        motor_a2.drive(DriveCommand::Stop).expect("driving");

        motor_b1.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("B1: {:?}", motor_b1.current_drive_command());
        delay.delay_millis(interval);
        motor_b1.drive(DriveCommand::Backward(speed)).expect("driving");
        println!("B1: {:?}", motor_b1.current_drive_command());
        delay.delay_millis(interval);
        motor_b1.drive(DriveCommand::Stop).expect("driving");

        motor_b2.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("B2: {:?}", motor_b2.current_drive_command());
        delay.delay_millis(interval);
        motor_b2.drive(DriveCommand::Backward(speed)).expect("driving");
        println!("B2: {:?}", motor_b2.current_drive_command());
        delay.delay_millis(interval);
        motor_b2.drive(DriveCommand::Stop).expect("driving");

        println!("Test patterns");
        motor_a1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Forward(speed)).expect("driving");

        delay.delay_millis(interval);

        motor_a1.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_b1.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Backward(speed)).expect("driving");

        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Stop).expect("driving");
        motor_a2.drive(DriveCommand::Stop).expect("driving");
        motor_b1.drive(DriveCommand::Stop).expect("driving");
        motor_b2.drive(DriveCommand::Stop).expect("driving");

        motor_a1.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Backward(speed)).expect("driving");

        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Stop).expect("driving");
        motor_a2.drive(DriveCommand::Stop).expect("driving");
        motor_b1.drive(DriveCommand::Stop).expect("driving");
        motor_b2.drive(DriveCommand::Stop).expect("driving");

        motor_a1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_b1.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Forward(speed)).expect("driving");

        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Stop).expect("driving");
        motor_a2.drive(DriveCommand::Stop).expect("driving");
        motor_b1.drive(DriveCommand::Stop).expect("driving");
        motor_b2.drive(DriveCommand::Stop).expect("driving");

        motor_a1.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b1.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Forward(speed)).expect("driving");

        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Stop).expect("driving");
        motor_a2.drive(DriveCommand::Stop).expect("driving");
        motor_b1.drive(DriveCommand::Stop).expect("driving");
        motor_b2.drive(DriveCommand::Stop).expect("driving");

        motor_a1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Backward(speed)).expect("driving");
        motor_b1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Backward(speed)).expect("driving");

        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Stop).expect("driving");
        motor_a2.drive(DriveCommand::Stop).expect("driving");
        motor_b1.drive(DriveCommand::Stop).expect("driving");
        motor_b2.drive(DriveCommand::Stop).expect("driving");

        motor_a1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_a2.drive(DriveCommand::Forward(speed / 2)).expect("driving");
        motor_b1.drive(DriveCommand::Forward(speed)).expect("driving");
        motor_b2.drive(DriveCommand::Forward(speed / 2)).expect("driving");

        delay.delay_millis(interval);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-beta.0/examples/src/bin
}

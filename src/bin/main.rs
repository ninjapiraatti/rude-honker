#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_println::println;
//use esp_hal::peripherals::MCPWM0;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    mcpwm::{operator::PwmPinConfig, timer::PwmWorkingMode, McPwm, PeripheralClockConfig},
    time::Rate
};
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::timer::timg::TimerGroup;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
use tb6612fng::{DriveCommand, Motor};

extern crate alloc;

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.3.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let _init = esp_wifi::init(
        timer1.timer0,
        esp_hal::rng::Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
    )
    .unwrap();

    // Motors
    let motor_a1_pin1 = Output::new(peripherals.GPIO22, Level::Low, OutputConfig::default());
    let motor_a1_pin2 = Output::new(peripherals.GPIO23, Level::Low, OutputConfig::default());
    let motor_a2_pin1 = Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());
    let motor_a2_pin2 = Output::new(peripherals.GPIO20, Level::Low, OutputConfig::default());
    let motor_b1_pin1 = Output::new(peripherals.GPIO9, Level::Low, OutputConfig::default());
    let motor_b1_pin2 = Output::new(peripherals.GPIO12, Level::Low, OutputConfig::default());
    let motor_b2_pin1 = Output::new(peripherals.GPIO18, Level::Low, OutputConfig::default());
    let motor_b2_pin2 = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let mut stdby_pin = Output::new(peripherals.GPIO11, Level::Low, OutputConfig::default());

    let motor_a1_pwm_pin = peripherals.GPIO15;
    let motor_a2_pwm_pin = peripherals.GPIO19;
    let motor_b1_pwm_pin = peripherals.GPIO13;
    let motor_b2_pwm_pin = peripherals.GPIO10;

    let clock_cfg = PeripheralClockConfig::with_frequency(Rate::from_mhz(40)).unwrap();
    let mut mcpwm_a = McPwm::new(peripherals.MCPWM0, clock_cfg);
    //let mut mcpwm_b = McPwm::new(MCPWM1, clock_cfg);
    mcpwm_a.operator0.set_timer(&mcpwm_a.timer0);
    mcpwm_a.operator1.set_timer(&mcpwm_a.timer1);
    //mcpwm_b.operator0.set_timer(&mcpwm_b.timer0);
    //mcpwm_b.operator1.set_timer(&mcpwm_b.timer1);
    let motor_a1_pwm = mcpwm_a.operator0.with_pin_a(motor_a1_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    let motor_a2_pwm = mcpwm_a.operator1.with_pin_a(motor_a2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    //let motor_b1_pwm = mcpwm_b.operator0.with_pin_a(motor_b1_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    //let motor_b2_pwm = mcpwm_b.operator1.with_pin_a(motor_b2_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    let mut motor_a1 = Motor::new(motor_a1_pin1, motor_a1_pin2, motor_a1_pwm).unwrap();
    let mut motor_a2 = Motor::new(motor_a2_pin1, motor_a2_pin2, motor_a2_pwm).unwrap();
    //let mut motor_b1 = Motor::new(motor_b1_pin1, motor_b1_pin2, motor_b1_pwm).unwrap();
    //let mut motor_b2 = Motor::new(motor_b2_pin1, motor_b2_pin2, motor_b2_pwm).unwrap();

    let timer_clock_cfg = clock_cfg
        .timer_clock_with_frequency(99, PwmWorkingMode::Increase, Rate::from_khz(20))
        .unwrap();
    mcpwm_a.timer0.start(timer_clock_cfg);
    mcpwm_a.timer1.start(timer_clock_cfg);
    //mcpwm_b.timer0.start(timer_clock_cfg);
    //mcpwm_b.timer1.start(timer_clock_cfg);
    stdby_pin.set_high();

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        //Timer::after(Duration::from_secs(1)).await;
        let delay = Delay::new();
        let interval = 1000;
        let speed = 100;

        motor_a1.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("Debug: {:?}", motor_a1.current_drive_command());
        delay.delay_millis(interval);
        motor_a1.drive(DriveCommand::Backward(speed)).expect("driving");
        delay.delay_millis(interval);

        motor_a2.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("Debug: {:?}", motor_a2.current_drive_command());
        delay.delay_millis(interval);
        motor_a2.drive(DriveCommand::Backward(speed)).expect("driving");
        delay.delay_millis(interval);

        /*
        motor_b1.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("Debug: {:?}", motor_b1.current_drive_command());
        delay.delay_millis(500);
        motor_b1.drive(DriveCommand::Backward(speed)).expect("driving");
        delay.delay_millis(500);

        motor_b2.drive(DriveCommand::Forward(speed)).expect("driving");
        println!("Debug: {:?}", motor_b2.current_drive_command());
        delay.delay_millis(500);
        motor_b2.drive(DriveCommand::Backward(speed)).expect("driving");
        delay.delay_millis(500);
        */
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-beta.0/examples/src/bin
}

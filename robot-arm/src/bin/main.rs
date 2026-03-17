#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::timer::systimer::SystemTimer;
use esp_println::println;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {:?}", info);
    loop {}
}

const SEQUENCE: [(bool, bool); 4] = [(false, false), (false, true), (true, true), (true, false)];

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    println!("Stepper varied test");

    let mut dir1 = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let mut dir2 = Output::new(peripherals.GPIO3, Level::Low, OutputConfig::default());
    let mut pwm1 = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let mut pwm2 = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    pwm1.set_high();
    pwm2.set_high();

    let mut step_index: usize = 0;

    macro_rules! step {
        ($forward:expr) => {{
            let (d1, d2) = SEQUENCE[step_index];
            if d1 {
                dir1.set_high();
            } else {
                dir1.set_low();
            }
            if d2 {
                dir2.set_high();
            } else {
                dir2.set_low();
            }
            if $forward {
                step_index = (step_index + 1) % 4;
            } else {
                step_index = (step_index + 3) % 4;
            }
        }};
    }

    loop {
        // Slow, short: 10 steps at 150ms
        println!("Slow short forward...");
        for _ in 0..10 {
            step!(true);
            Timer::after(Duration::from_millis(120)).await;
        }
        Timer::after(Duration::from_millis(500)).await;

        // Fast, short: 10 steps at 10ms
        println!("Fast long forward...");
        for _ in 0..1000 {
            step!(true);
            Timer::after(Duration::from_millis(2)).await;
        }
        Timer::after(Duration::from_millis(500)).await;

        // Slow, long: half revolution backward at 50ms
        println!("Slow long backward (half rev)...");
        for _ in 0..100 {
            step!(false);
            Timer::after(Duration::from_millis(60)).await;
        }
        Timer::after(Duration::from_millis(500)).await;

        // Fast backward half rev
        println!("Fast backward (half rev)...");
        for _ in 0..100 {
            step!(false);
            Timer::after(Duration::from_millis(12)).await;
        }
        Timer::after(Duration::from_millis(500)).await;

        // Quick bursts: 5 steps each direction x4
        println!("Quick bursts...");
        for _ in 0..4 {
            for _ in 0..5 {
                step!(true);
                Timer::after(Duration::from_millis(20)).await;
            }
            Timer::after(Duration::from_millis(200)).await;
            for _ in 0..5 {
                step!(false);
                Timer::after(Duration::from_millis(20)).await;
            }
            Timer::after(Duration::from_millis(200)).await;
        }

        println!("--- Cycle complete ---\n");
        Timer::after(Duration::from_secs(2)).await;
    }
}

#![no_std]
#![no_main]

use esp_hal::clock::CpuClock;
use esp_println::println;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[esp_riscv_rt::entry]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);

    println!("Robot arm starting...");

    loop {}
}

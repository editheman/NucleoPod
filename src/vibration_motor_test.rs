#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("Motor vibratii test pornit");

    let mut motor = Output::new(p.PB10, Level::Low, Speed::Low);

    loop {
        info!("Motor ON");
        motor.set_high();
        Timer::after_millis(500).await;

        info!("Motor OFF");
        motor.set_low();
        Timer::after_millis(500).await;
    }
}
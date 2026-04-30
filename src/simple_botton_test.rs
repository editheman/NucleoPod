#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Pull};
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("Buton test pornit");

    let button = Input::new(p.PB4, Pull::Up);

    loop {
        if button.is_low() {
            info!("Buton apasat!");
        }
        Timer::after_millis(125).await;
    }
}
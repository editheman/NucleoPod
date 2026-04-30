#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::bind_interrupts;
use embassy_stm32::i2c::{self, I2c};
use embassy_stm32::peripherals;
use embassy_time::Timer;
use panic_probe as _;

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

const MPR121_ADDR: u8 = 0x5A;
const MPR121_SRST: u8 = 0x80; // soft reset register
const MPR121_SRST_VAL: u8 = 0x63; // soft reset value

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("MPR121 test pornit");

    let mut i2c = I2c::new(
    p.I2C1,
    p.PB6,
    p.PB7,
    Irqs,
    p.GPDMA1_CH0,
    p.GPDMA1_CH1,
    Default::default(),
);

    // Soft reset MPR121
    i2c.write(MPR121_ADDR, &[MPR121_SRST, MPR121_SRST_VAL])
        .await
        .unwrap();

    Timer::after_millis(10).await;

    // Citeste registrul 0x5D (chip ID ar trebui sa fie 0x24)
    let mut buf = [0u8; 1];
    i2c.write_read(MPR121_ADDR, &[0x5D], &mut buf)
        .await
        .unwrap();

    info!("MPR121 raspuns registru 0x5D: 0x{:02X}", buf[0]);

    if buf[0] == 0x24 {
        info!("MPR121 detectat corect!");
    } else {
        info!("Raspuns neasteptat - verifica cablajul");
    }

    loop {
        Timer::after_secs(1).await;
    }
}
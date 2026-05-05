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

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("MPR121 touch test pornit");

    let mut i2c = I2c::new(
        p.I2C1,
        p.PB6,
        p.PB7,
        Irqs,
        p.GPDMA1_CH0,
        p.GPDMA1_CH1,
        Default::default(),
    );

    i2c.write(MPR121_ADDR, &[0x80, 0x63]).await.unwrap();
    Timer::after_millis(10).await;

    i2c.write(MPR121_ADDR, &[0x2B, 0x01]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x2C, 0x01]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x2D, 0x00]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x2E, 0x00]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x2F, 0x01]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x30, 0x01]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x31, 0xFF]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x32, 0x02]).await.unwrap();

    for i in 0..8u8 {
        i2c.write(MPR121_ADDR, &[0x41 + i * 2, 0x06]).await.unwrap();
        i2c.write(MPR121_ADDR, &[0x42 + i * 2, 0x03]).await.unwrap();
    }

    i2c.write(MPR121_ADDR, &[0x5C, 0x10]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x5D, 0x20]).await.unwrap();
    i2c.write(MPR121_ADDR, &[0x5E, 0x08]).await.unwrap();

    info!("MPR121 configurat, atinge electrozii!");

    loop {
        let mut raw = [0u8; 2];
        i2c.write_read(MPR121_ADDR, &[0x04], &mut raw).await.unwrap();
        let val = (raw[0] as u16) | ((raw[1] as u16) << 8);
        info!("Electrod 0 valoare bruta: {}", val);

        let mut buf = [0u8; 2];
        i2c.write_read(MPR121_ADDR, &[0x00], &mut buf).await.unwrap();
        let touched = (buf[0] as u16) | ((buf[1] as u16) << 8);

        for i in 0..8u8 {
            if touched & (1 << i) != 0 {
                info!("Electrod {} atins!", i);
            }
        }

        Timer::after_millis(200).await;
    }
}
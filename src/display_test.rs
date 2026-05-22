#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Delay, Timer};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use embedded_hal_bus::spi::ExclusiveDevice;
use mipidsi::Builder;
use mipidsi::models::ILI9341Rgb565;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("Display test pornit");

    let mut spi_config = Config::default();
    spi_config.frequency = Hertz(1_000_000);

    let spi = Spi::new_blocking_txonly(
        p.SPI1,
        p.PA5,
        p.PA7,
        spi_config,
    );

    let cs = Output::new(p.PA6, Level::High, Speed::High);
    let dc = Output::new(p.PC7, Level::Low, Speed::High);
    let rst = Output::new(p.PC6, Level::Low, Speed::High);

    let spi_dev = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

    let mut buffer = [0u8; 320];
    let di = mipidsi::interface::SpiInterface::new(spi_dev, dc, &mut buffer);

    let mut delay = Delay;
    let mut display = Builder::new(ILI9341Rgb565, di)
        .reset_pin(rst)
        .orientation(mipidsi::options::Orientation::new().flip_horizontal())
        .init(&mut delay)
        .unwrap();

    display.clear(Rgb565::BLACK).unwrap();

    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    Text::new("NucleoPod", Point::new(80, 120), style)
        .draw(&mut display)
        .unwrap();

    info!("Display initializat");

    loop {
        Timer::after_secs(1).await;
    }
}
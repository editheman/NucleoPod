#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Delay, Timer};
use embedded_sdmmc::{SdCard, VolumeManager};
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("SD card test pornit");

    let mut spi_config = Config::default();
    spi_config.frequency = Hertz(1_000_000);

    let spi = Spi::new_blocking(
        p.SPI2,
        p.PB13,
        p.PB15,
        p.PB14,
        spi_config,
    );

    let cs = Output::new(p.PB5, Level::High, Speed::High);
    let sdcard = SdCard::new(spi, cs, Delay);

    info!("Initializez SD card...");

    let mut volume_mgr = VolumeManager::new(sdcard, DummyTimesource);

    match volume_mgr.open_volume(embedded_sdmmc::VolumeIdx(0)) {
        Ok(volume) => {
            info!("Volum deschis!");
            match volume_mgr.open_root_dir(volume) {
                Ok(root_dir) => {
                    info!("Listez fisiere:");
                    volume_mgr.iterate_dir(root_dir, |entry| {
                        if !entry.attributes.is_hidden() {
                            // ShortFileName nu implementeaza Format, convertim la bytes
                            let name = entry.name.base_name();
                            info!("  {}", core::str::from_utf8(name).unwrap_or("???"));
                        }
                    }).unwrap();
                    volume_mgr.close_dir(root_dir).unwrap();
                }
                Err(_) => info!("Eroare root dir"),
            }
            volume_mgr.close_volume(volume).unwrap();
        }
        Err(_) => info!("Eroare volum"),
    }

    loop {
        Timer::after_secs(1).await;
    }
}

struct DummyTimesource;

impl embedded_sdmmc::TimeSource for DummyTimesource {
    fn get_timestamp(&self) -> embedded_sdmmc::Timestamp {
        embedded_sdmmc::Timestamp {
            year_since_1970: 54,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}
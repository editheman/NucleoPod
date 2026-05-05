#![no_std]
#![no_main]

use cortex_m::asm::delay;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::OutputType;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::Channel;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_time::Timer;
use embedded_hal::Pwm;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{SdCard, VolumeManager};
use panic_probe as _;

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

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("NucleoPod WAV player pornit");

    let ch1 = PwmPin::new(p.PA0, OutputType::PushPull);
    let mut pwm = SimplePwm::new(
        p.TIM2,
        Some(ch1),
        None,
        None,
        None,
        Hertz(100_000),
        Default::default(),
    );
    let max = pwm.get_max_duty();
    pwm.set_duty(Channel::Ch1, max / 2);
    pwm.enable(Channel::Ch1);

    let mut spi_config = Config::default();
    spi_config.frequency = Hertz(4_000_000);

    let spi = Spi::new_blocking(p.SPI2, p.PB13, p.PB15, p.PB14, spi_config);

    let cs = Output::new(p.PB5, Level::High, Speed::High);
    let sdcard = SdCard::new(spi, cs, embassy_time::Delay);

    let mut volume_mgr = VolumeManager::new(sdcard, DummyTimesource);

    let volume = volume_mgr
        .open_volume(embedded_sdmmc::VolumeIdx(0))
        .unwrap();
    let root_dir = volume_mgr.open_root_dir(volume).unwrap();

    let file = volume_mgr
        .open_file_in_dir(root_dir, "BONITA.WAV", embedded_sdmmc::Mode::ReadOnly)
        .unwrap();

    info!("Fisier deschis, redau...");

    let mut header = [0u8; 44];
    volume_mgr.read(file, &mut header).unwrap();

    let mut buf = [0u8; 1024];

    let t1 = embassy_time::Instant::now();
    let bytes_read = volume_mgr.read(file, &mut buf).unwrap();
    let t2 = embassy_time::Instant::now();
    info!("Citire SD: {}ms", (t2 - t1).as_millis());

    loop {
        let bytes_read = volume_mgr.read(file, &mut buf).unwrap();
        if bytes_read == 0 {
            info!("Fisier terminat!");
            break;
        }

        for i in 0..bytes_read {
            let sample = buf[i] as u32;
            let duty = (sample * max as u32 / 255) as u32;
            pwm.set_duty(Channel::Ch1, duty);
            delay(220);
        }
    }

    info!("Redare terminata!");

    loop {
        Timer::after_secs(1).await;
    }
}

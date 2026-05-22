#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use cortex_m::asm::delay as asm_delay;
use embassy_executor::Spawner;
use embassy_stm32::dac::{DacCh1, Value};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::mux::Dacsel;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::Config as StmConfig;
use embassy_time::Timer;
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
    let mut config = StmConfig::default();
    config.rcc.mux.dac1sel = Dacsel::LSE;
    config.rcc.ls = embassy_stm32::rcc::LsConfig::default_lse();
    config.rcc.hsi = true;
    config.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_R;
    config.rcc.pll1 = Some(embassy_stm32::rcc::Pll {
        source: embassy_stm32::rcc::PllSource::HSI,
        prediv: embassy_stm32::rcc::PllPreDiv::DIV1,
        mul: embassy_stm32::rcc::PllMul::MUL10,
        divp: None,
        divq: None,
        divr: Some(embassy_stm32::rcc::PllDiv::DIV1),
    });
    let p = embassy_stm32::init(config);
    info!("NucleoPod DAC player pornit");

    Timer::after_millis(500).await;

    let mut dac = DacCh1::new(p.DAC1, p.GPDMA1_CH0, p.PA4);
    dac.enable();

    let mut spi_config = Config::default();
    spi_config.frequency = Hertz(8_000_000);

    let spi = Spi::new_blocking(
        p.SPI2,
        p.PB13,
        p.PB15,
        p.PB14,
        spi_config,
    );

    let cs = Output::new(p.PB5, Level::High, Speed::High);
    let sdcard = SdCard::new(spi, cs, embassy_time::Delay);
    let mut volume_mgr = VolumeManager::new(sdcard, DummyTimesource);

    let volume = volume_mgr.open_volume(embedded_sdmmc::VolumeIdx(0)).unwrap();
    let root_dir = volume_mgr.open_root_dir(volume).unwrap();
    let file = volume_mgr.open_file_in_dir(
        root_dir,
        "BONITA.WAV",
        embedded_sdmmc::Mode::ReadOnly,
    ).unwrap();

    info!("Fisier deschis, redau...");

    let mut header = [0u8; 44];
    volume_mgr.read(file, &mut header).unwrap();

    let mut buf_a = [0u8; 8192];
    let mut buf_b = [0u8; 8192];

    let mut bytes_play = volume_mgr.read(file, &mut buf_a).unwrap();
    let mut use_a = true;

    loop {
        if use_a {
            let mut bytes_next = 0;
            for i in 0..bytes_play {
                let sample = buf_a[i] as u16;
                dac.set(Value::Bit12Right(sample * 16));
                asm_delay(650);
                if i == bytes_play / 4 {
                    bytes_next = volume_mgr.read(file, &mut buf_b).unwrap();
                }
            }
            if bytes_next == 0 { break; }
            bytes_play = bytes_next;
            use_a = false;
        } else {
            let mut bytes_next = 0;
            for i in 0..bytes_play {
                let sample = buf_b[i] as u16;
                dac.set(Value::Bit12Right(sample * 16));
                asm_delay(650);
                if i == bytes_play / 4 {
                    bytes_next = volume_mgr.read(file, &mut buf_a).unwrap();
                }
            }
            if bytes_next == 0 { break; }
            bytes_play = bytes_next;
            use_a = true;
        }
    }

    info!("Redare terminata!");
    loop {
        Timer::after_secs(1).await;
    }
}
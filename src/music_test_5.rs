#![no_std]
#![no_main]

use cortex_m::asm::delay as asm_delay;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::Config as StmConfig;
use embassy_stm32::dac::DacCh1;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::mux::Dacsel;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use embedded_sdmmc::{SdCard, VolumeManager};
use panic_probe as _;

const BUF_SIZE: usize = 16384;

static CHANNEL: Channel<CriticalSectionRawMutex, ([u8; BUF_SIZE], usize), 2> = Channel::new();

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

#[embassy_executor::task]
async fn audio_task() {
    let dac_regs = embassy_stm32::pac::DAC1;
    loop {
        let (buf, len) = CHANNEL.receive().await;
        for i in 0..len {
            let val = (buf[i] as u32) * 16;
            dac_regs.dhr12r(0).write(|w| w.set_dhr(val as u16));
            asm_delay(650);
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
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

    let mut header = [0u8; 44];
    volume_mgr.read(file, &mut header).unwrap();

    spawner.spawn(audio_task()).unwrap();

    for _ in 0..2 {
        let mut send_buf = [0u8; BUF_SIZE];
        let n = volume_mgr.read(file, &mut send_buf).unwrap();
        if n == 0 {
            break;
        }
        CHANNEL.send((send_buf, n)).await;
    }

    info!("Citesc si trimit buffere...");

    loop {
        let mut send_buf = [0u8; BUF_SIZE];
        let n = volume_mgr.read(file, &mut send_buf).unwrap();
        if n == 0 {
            info!("Fisier terminat!");
            break;
        }
        CHANNEL.send((send_buf, n)).await;
    }

    loop {
        Timer::after_secs(1).await;
    }
}

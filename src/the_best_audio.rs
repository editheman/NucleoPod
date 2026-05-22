#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use cortex_m::peripheral::NVIC;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::dac::DacCh1;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::pac;
use embassy_stm32::pac::interrupt as PacInterrupt;
use embassy_stm32::interrupt; // Fix 1: Importam macro-ul corect
use embassy_stm32::pac::timer::regs::ArrCore; // Fix 2: Importam tipul strict pentru registrul ARR

use embassy_stm32::rcc::mux::Dacsel;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::Config as StmConfig;
use embassy_time::Timer;
use embedded_sdmmc::{SdCard, VolumeManager};
use panic_probe as _;

const BUF_SIZE: usize = 8192;

static mut BUF_A: [u8; BUF_SIZE] = [128u8; BUF_SIZE];
static mut BUF_B: [u8; BUF_SIZE] = [128u8; BUF_SIZE];

static PLAY_IDX: AtomicU32 = AtomicU32::new(0);
static PLAY_LEN: AtomicU32 = AtomicU32::new(0);
static USE_BUF_A: AtomicBool = AtomicBool::new(true);
static BUF_READY: AtomicBool = AtomicBool::new(false);
static NEXT_LEN: AtomicU32 = AtomicU32::new(0);

#[interrupt]
fn TIM6() {
    unsafe {
        pac::TIM6.sr().modify(|w| w.set_uif(false));

        let idx = PLAY_IDX.load(Ordering::Relaxed);
        let len = PLAY_LEN.load(Ordering::Relaxed);

        if idx >= len {
            if BUF_READY.load(Ordering::Relaxed) {
                let next_len = NEXT_LEN.load(Ordering::Relaxed);
                let was_a = USE_BUF_A.load(Ordering::Relaxed);
                USE_BUF_A.store(!was_a, Ordering::Relaxed);
                PLAY_LEN.store(next_len, Ordering::Relaxed);
                PLAY_IDX.store(0, Ordering::Relaxed);
                BUF_READY.store(false, Ordering::Relaxed);
            }
            return;
        }

        let use_a = USE_BUF_A.load(Ordering::Relaxed);
        let sample = if use_a {
            BUF_A[idx as usize]
        } else {
            BUF_B[idx as usize]
        };

        let val = (sample as u16) * 16;
        pac::DAC1.dhr12r(0).write(|w| w.set_dhr(val));

        PLAY_IDX.store(idx + 1, Ordering::Relaxed);
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

fn setup_tim6(sample_rate: u32, timer_clock: u32) {
    pac::RCC.apb1enr1().modify(|w| w.set_tim6en(true));

    let tim = pac::TIM6;
    let arr = (timer_clock / sample_rate) - 1;

    tim.psc().write_value(0);
    // Folosim structura ArrCore ceruta de versiunea 19 de metapac
    tim.arr().write_value(ArrCore(arr)); 
    tim.dier().write(|w| w.set_uie(true));
    tim.cr1().write(|w| {
        w.set_arpe(true);
        w.set_cen(true);
    });

    unsafe {
        NVIC::unmask(PacInterrupt::TIM6);
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
    info!("NucleoPod TIM6 player pornit");

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

    info!("Fisier deschis");

    let mut header = [0u8; 44];
    volume_mgr.read(file, &mut header).unwrap();

    let n = unsafe {
        volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap()
    };
    PLAY_LEN.store(n as u32, Ordering::Relaxed);
    USE_BUF_A.store(true, Ordering::Relaxed);
    PLAY_IDX.store(0, Ordering::Relaxed);

    setup_tim6(44100, 160_000_000);
    info!("Redare pornita cu TIM6!");

    loop {
        while BUF_READY.load(Ordering::Relaxed) {
            Timer::after_micros(500).await;
        }

        let active_a = USE_BUF_A.load(Ordering::Relaxed);
        let n = unsafe {
            if active_a {
                volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_B)).unwrap()
            } else {
                volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap()
            }
        };

        if n == 0 {
            info!("Fisier terminat!");
            break;
        }

        NEXT_LEN.store(n as u32, Ordering::Relaxed);
        BUF_READY.store(true, Ordering::Relaxed);
    }

    loop {
        Timer::after_secs(1).await;
    }
}
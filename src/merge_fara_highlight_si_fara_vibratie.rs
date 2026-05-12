#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use cortex_m::peripheral::NVIC;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::dac::DacCh1;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::{self, I2c};
use embassy_stm32::bind_interrupts;
use embassy_stm32::peripherals;
use embassy_stm32::pac;
use embassy_stm32::pac::interrupt as PacInterrupt;
use embassy_stm32::interrupt;
use embassy_stm32::pac::timer::regs::ArrCore;
use embassy_stm32::rcc::mux::Dacsel;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::Config as StmConfig;
use embassy_time::{Delay, Timer};
use embedded_graphics::mono_font::ascii::{FONT_9X15, FONT_9X15_BOLD};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use embedded_graphics::primitives::{Rectangle, PrimitiveStyle};
use embedded_hal_bus::spi::ExclusiveDevice;
use mipidsi::Builder;
use mipidsi::models::ILI9341Rgb565;
use embedded_sdmmc::{SdCard, VolumeManager, ShortFileName};
use heapless::Vec;
use heapless::String;
use panic_probe as _;

const BUF_SIZE: usize = 8192;
const MAX_FILES: usize = 16;

static mut BUF_A: [u8; BUF_SIZE] = [128u8; BUF_SIZE];
static mut BUF_B: [u8; BUF_SIZE] = [128u8; BUF_SIZE];

static PLAY_IDX: AtomicU32 = AtomicU32::new(0);
static PLAY_LEN: AtomicU32 = AtomicU32::new(0);
static USE_BUF_A: AtomicBool = AtomicBool::new(true);
static BUF_READY: AtomicBool = AtomicBool::new(false);
static NEXT_LEN: AtomicU32 = AtomicU32::new(0);
static AUDIO_PLAYING: AtomicBool = AtomicBool::new(false);
static FILE_DONE: AtomicBool = AtomicBool::new(false);

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

#[interrupt]
fn TIM6() {
    unsafe {
        pac::TIM6.sr().modify(|w| w.set_uif(false));

        if !AUDIO_PLAYING.load(Ordering::Relaxed) {
            return;
        }

        let idx = PLAY_IDX.load(Ordering::Relaxed);
        let len = PLAY_LEN.load(Ordering::Relaxed);

        if idx >= len {
            if BUF_READY.load(Ordering::Relaxed) {
                let next_len = NEXT_LEN.load(Ordering::Relaxed);
                if next_len == 0 {
                    // Fisier terminat
                    AUDIO_PLAYING.store(false, Ordering::Relaxed);
                    FILE_DONE.store(true, Ordering::Relaxed);
                    BUF_READY.store(false, Ordering::Relaxed);
                    return;
                }
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

#[derive(PartialEq)]
enum AppState {
    Menu,
    Playing,
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
    info!("NucleoPod pornit");

    Timer::after_millis(500).await;

    let mut dac = DacCh1::new(p.DAC1, p.GPDMA1_CH0, p.PA4);
    dac.enable();

    setup_tim6(44100, 160_000_000);

    // Display SPI1
    let mut spi1_config = SpiConfig::default();
    spi1_config.frequency = Hertz(1_000_000);
    let spi1 = Spi::new_blocking_txonly(p.SPI1, p.PA5, p.PA7, spi1_config);
    let cs_display = Output::new(p.PC9, Level::High, Speed::High);
    let dc = Output::new(p.PC7, Level::Low, Speed::High);
    let rst = Output::new(p.PC6, Level::Low, Speed::High);
    let spi1_dev = ExclusiveDevice::new_no_delay(spi1, cs_display).unwrap();
    let mut buffer = [0u8; 320];
    let di = mipidsi::interface::SpiInterface::new(spi1_dev, dc, &mut buffer);
    let mut display = Builder::new(ILI9341Rgb565, di)
        .reset_pin(rst)
        .orientation(mipidsi::options::Orientation::new().flip_horizontal())
        .init(&mut Delay)
        .unwrap();
    display.clear(Rgb565::BLACK).unwrap();

    // SD SPI2
    let mut spi2_config = SpiConfig::default();
    spi2_config.frequency = Hertz(8_000_000);
    let spi2 = Spi::new_blocking(p.SPI2, p.PB13, p.PB15, p.PB14, spi2_config);
    let cs_sd = Output::new(p.PB5, Level::High, Speed::High);
    let sdcard = SdCard::new(spi2, cs_sd, embassy_time::Delay);
    let mut volume_mgr = VolumeManager::new(sdcard, DummyTimesource);

    // I2C MPR121
    let mut i2c = I2c::new(
        p.I2C1, p.PB6, p.PB7, Irqs,
        p.GPDMA1_CH1, p.GPDMA1_CH2,
        Default::default(),
    );

    let mut motor = Output::new(p.PB10, Level::Low, Speed::Low);
    let button = embassy_stm32::gpio::Input::new(p.PB4, embassy_stm32::gpio::Pull::Up);

    // Configureaza MPR121
    i2c.write(0x5A, &[0x80, 0x63]).await.unwrap();
    Timer::after_millis(10).await;
    for i in 0..8u8 {
        i2c.write(0x5A, &[0x41 + i * 2, 0x06]).await.unwrap();
        i2c.write(0x5A, &[0x42 + i * 2, 0x03]).await.unwrap();
    }
    i2c.write(0x5A, &[0x5C, 0x10]).await.unwrap();
    i2c.write(0x5A, &[0x5D, 0x20]).await.unwrap();
    i2c.write(0x5A, &[0x5E, 0x08]).await.unwrap();

    // Citeste lista fisiere
    let volume = volume_mgr.open_volume(embedded_sdmmc::VolumeIdx(0)).unwrap();
    let root_dir = volume_mgr.open_root_dir(volume).unwrap();

    let mut file_names: Vec<ShortFileName, MAX_FILES> = Vec::new();
    volume_mgr.iterate_dir(root_dir, |entry| {
        if !entry.attributes.is_directory() && !entry.attributes.is_hidden() {
            let ext = entry.name.extension();
            if ext == b"WAV" || ext == b"wav" {
                let _ = file_names.push(entry.name.clone());
            }
        }
    }).unwrap();

    info!("Gasit {} fisiere WAV", file_names.len());

    let mut selected: usize = 0;
    let mut current_idx: usize = 0;
    let mut app_state = AppState::Menu;
    let mut last_touched: u16 = 0;
    let mut current_file: Option<embedded_sdmmc::File> = None;

    draw_menu(&mut display, &file_names, selected);

    loop {
        // Citire continua SD cand se reda
        if app_state == AppState::Playing {
            if !BUF_READY.load(Ordering::Relaxed) && AUDIO_PLAYING.load(Ordering::Relaxed) {
                if let Some(ref file) = current_file {
                    let active_a = USE_BUF_A.load(Ordering::Relaxed);
                    let n = unsafe {
                        if active_a {
                            volume_mgr.read(*file, &mut *core::ptr::addr_of_mut!(BUF_B)).unwrap_or(0)
                        } else {
                            volume_mgr.read(*file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap_or(0)
                        }
                    };
                    NEXT_LEN.store(n as u32, Ordering::Relaxed);
                    BUF_READY.store(true, Ordering::Relaxed);
                }
            }

            // Fisier terminat - trece la urmatorul
            if FILE_DONE.load(Ordering::Relaxed) {
                FILE_DONE.store(false, Ordering::Relaxed);
                current_idx = (current_idx + 1) % file_names.len();
                selected = current_idx;

                // Deschide urmatorul fisier
                if let Some(f) = current_file {
                    volume_mgr.close_file(f).ok();
                }
                let file = volume_mgr.open_file_in_dir(
                    root_dir,
                    &file_names[current_idx],
                    embedded_sdmmc::Mode::ReadOnly,
                ).unwrap();
                let mut header = [0u8; 44];
                volume_mgr.read(file, &mut header).unwrap();
                let n = unsafe {
                    volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap()
                };
                current_file = Some(file);
                PLAY_LEN.store(n as u32, Ordering::Relaxed);
                USE_BUF_A.store(true, Ordering::Relaxed);
                PLAY_IDX.store(0, Ordering::Relaxed);
                BUF_READY.store(false, Ordering::Relaxed);
                AUDIO_PLAYING.store(true, Ordering::Relaxed);
                draw_player(&mut display, &file_names[current_idx]);
                info!("Urmatoarea melodie: {}", current_idx);
            }
        }

        // Citeste MPR121
        let mut buf = [0u8; 2];
        if i2c.write_read(0x5A, &[0x00], &mut buf).await.is_ok() {
            let touched = (buf[0] as u16) | ((buf[1] as u16) << 8);
            let new_touch = touched & !last_touched;

            if new_touch & (1 << 0) != 0 {
                motor.set_high();
                Timer::after_millis(30).await;
                motor.set_low();
                match app_state {
                    AppState::Menu => {
                        if selected > 0 { selected -= 1; }
                        draw_menu(&mut display, &file_names, selected);
                    }
                    AppState::Playing => {}
                }
            }

            if new_touch & (1 << 4) != 0 {
                motor.set_high();
                Timer::after_millis(30).await;
                motor.set_low();
                match app_state {
                    AppState::Menu => {
                        if selected < file_names.len().saturating_sub(1) { selected += 1; }
                        draw_menu(&mut display, &file_names, selected);
                    }
                    AppState::Playing => {}
                }
            }

            last_touched = touched;
        }

        // Buton central
        if button.is_low() {
            Timer::after_millis(50).await;
            if button.is_low() {
                match app_state {
                    AppState::Menu => {
                        if !file_names.is_empty() {
                            AUDIO_PLAYING.store(false, Ordering::Relaxed);
                            Timer::after_millis(10).await;

                            if let Some(f) = current_file {
                                volume_mgr.close_file(f).ok();
                            }

                            let file = volume_mgr.open_file_in_dir(
                                root_dir,
                                &file_names[selected],
                                embedded_sdmmc::Mode::ReadOnly,
                            ).unwrap();
                            let mut header = [0u8; 44];
                            volume_mgr.read(file, &mut header).unwrap();
                            let n = unsafe {
                                volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap()
                            };
                            current_file = Some(file);
                            current_idx = selected;
                            PLAY_LEN.store(n as u32, Ordering::Relaxed);
                            USE_BUF_A.store(true, Ordering::Relaxed);
                            PLAY_IDX.store(0, Ordering::Relaxed);
                            BUF_READY.store(false, Ordering::Relaxed);
                            FILE_DONE.store(false, Ordering::Relaxed);
                            AUDIO_PLAYING.store(true, Ordering::Relaxed);
                            app_state = AppState::Playing;
                            draw_player(&mut display, &file_names[selected]);
                        }
                    }
                    AppState::Playing => {
                        let playing = AUDIO_PLAYING.load(Ordering::Relaxed);
                        AUDIO_PLAYING.store(!playing, Ordering::Relaxed);
                    }
                }
                while button.is_low() {
                    Timer::after_millis(10).await;
                }
            }
        }

        Timer::after_millis(20).await;
    }
}

fn draw_menu<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    files: &Vec<ShortFileName, MAX_FILES>,
    selected: usize,
) {
    display.clear(Rgb565::BLACK).ok();
    let title_style = MonoTextStyle::new(&FONT_9X15_BOLD, Rgb565::CYAN);
    Text::new("NucleoPod", Point::new(90, 20), title_style).draw(display).ok();

    for (i, name) in files.iter().enumerate() {
        let y = 50 + i as i32 * 25;
        let is_selected = i == selected;

        if is_selected {
            Rectangle::new(Point::new(5, y - 15), Size::new(310, 22))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED))
                .draw(display).ok();
        }

        let color = if is_selected { Rgb565::WHITE } else { Rgb565::CSS_STEEL_BLUE };
        let style = MonoTextStyle::new(&FONT_9X15, color);
        let name_bytes = name.base_name();
        let name_str = core::str::from_utf8(name_bytes).unwrap_or("???");
        let display_str: String<20> = String::try_from(name_str).unwrap_or_default();
        Text::new(&display_str, Point::new(10, y), style).draw(display).ok();
    }
}

fn draw_player<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    filename: &ShortFileName,
) {
    display.clear(Rgb565::BLACK).ok();
    let title_style = MonoTextStyle::new(&FONT_9X15_BOLD, Rgb565::CSS_GOLD);
    Text::new("Now Playing", Point::new(80, 30), title_style).draw(display).ok();

    let name_bytes = filename.base_name();
    let name_str = core::str::from_utf8(name_bytes).unwrap_or("???");
    let display_str: String<20> = String::try_from(name_str).unwrap_or_default();
    let style = MonoTextStyle::new(&FONT_9X15, Rgb565::WHITE);
    Text::new(&display_str, Point::new(10, 120), style).draw(display).ok();
    Text::new("[ << ]  [ || ]  [ >> ]", Point::new(10, 200), style).draw(display).ok();
}
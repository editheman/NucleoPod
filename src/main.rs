#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::fmt::Write;
use cortex_m::peripheral::NVIC;
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
// Am importat Builder-ul pentru a scrie text fara flicker!
use embedded_graphics::mono_font::{MonoTextStyle, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use embedded_graphics::primitives::{Rectangle, Circle, PrimitiveStyle};
use embedded_graphics::image::Image;
use tinybmp::Bmp;
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

static mut IMAGE_BUF: [u8; 45000] = [0u8; 45000];
static mut CURRENT_BMP_LEN: usize = 0;

static mut FILE_SIZES: [u32; MAX_FILES] = [0; MAX_FILES];
static PLAYED_BYTES: AtomicU32 = AtomicU32::new(0);

// NOU: Variabila care tine minte pozitia veche a bulinei pentru anti-flicker
static mut LAST_FILL_WIDTH: u32 = 0;

static PLAY_IDX: AtomicU32 = AtomicU32::new(0);
static PLAY_LEN: AtomicU32 = AtomicU32::new(0);
static USE_BUF_A: AtomicBool = AtomicBool::new(true);
static BUF_READY: AtomicBool = AtomicBool::new(false);
static NEXT_LEN: AtomicU32 = AtomicU32::new(0);
static AUDIO_PLAYING: AtomicBool = AtomicBool::new(false);
static FILE_DONE: AtomicBool = AtomicBool::new(false);
static HAPTIC_TRIGGER: AtomicBool = AtomicBool::new(false);

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

#[interrupt]
fn TIM6() {
    unsafe {
        pac::TIM6.sr().modify(|w| w.set_uif(false));
        if !AUDIO_PLAYING.load(Ordering::Relaxed) { return; }
        let idx = PLAY_IDX.load(Ordering::Relaxed);
        let len = PLAY_LEN.load(Ordering::Relaxed);
        if idx >= len {
            if BUF_READY.load(Ordering::Relaxed) {
                let next_len = NEXT_LEN.load(Ordering::Relaxed);
                if next_len == 0 {
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
        let sample = if use_a { BUF_A[idx as usize] } else { BUF_B[idx as usize] };
        let val = (sample as u16) * 16;
        pac::DAC1.dhr12r(0).write(|w| w.set_dhr(val));
        PLAY_IDX.store(idx + 1, Ordering::Relaxed);
    }
}

#[embassy_executor::task]
async fn haptic_task(mut motor: Output<'static>) {
    loop {
        if HAPTIC_TRIGGER.load(Ordering::Relaxed) {
            HAPTIC_TRIGGER.store(false, Ordering::Relaxed);
            motor.set_high();
            Timer::after_millis(80).await;
            motor.set_low();
        }
        Timer::after_millis(5).await;
    }
}

struct DummyTimesource;
impl embedded_sdmmc::TimeSource for DummyTimesource {
    fn get_timestamp(&self) -> embedded_sdmmc::Timestamp {
        embedded_sdmmc::Timestamp { year_since_1970: 54, zero_indexed_month: 0, zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
    }
}

fn setup_tim6(sample_rate: u32, timer_clock: u32) {
    pac::RCC.apb1enr1().modify(|w| w.set_tim6en(true));
    let tim = pac::TIM6;
    let arr = (timer_clock / sample_rate) - 1;
    tim.psc().write_value(0);
    tim.arr().write_value(ArrCore(arr));
    tim.dier().write(|w| w.set_uie(true));
    tim.cr1().write(|w| { w.set_arpe(true); w.set_cen(true); });
    unsafe { NVIC::unmask(PacInterrupt::TIM6); }
}

#[derive(PartialEq)]
enum AppState { Menu, Playing }

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = StmConfig::default();
    config.rcc.mux.dac1sel = Dacsel::LSE;
    config.rcc.ls = embassy_stm32::rcc::LsConfig::default_lse();
    config.rcc.hsi = true;
    config.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_R;
    config.rcc.pll1 = Some(embassy_stm32::rcc::Pll {
        source: embassy_stm32::rcc::PllSource::HSI, prediv: embassy_stm32::rcc::PllPreDiv::DIV1,
        mul: embassy_stm32::rcc::PllMul::MUL10, divp: None, divq: None, divr: Some(embassy_stm32::rcc::PllDiv::DIV1),
    });
    let p = embassy_stm32::init(config);
    
    let mut dac = DacCh1::new(p.DAC1, p.GPDMA1_CH0, p.PA4);
    dac.enable();
    setup_tim6(44100, 160_000_000);

    let mut spi1_config = SpiConfig::default();
    spi1_config.frequency = Hertz(32_000_000); 
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
        .init(&mut Delay).unwrap();
    display.clear(Rgb565::BLACK).unwrap();

    let mut spi2_config = SpiConfig::default();
    spi2_config.frequency = Hertz(24_000_000);
    let spi2 = Spi::new_blocking(p.SPI2, p.PB13, p.PB15, p.PB14, spi2_config);
    let cs_sd = Output::new(p.PB5, Level::High, Speed::High);
    let sdcard = SdCard::new(spi2, cs_sd, embassy_time::Delay);
    let mut volume_mgr = VolumeManager::new(sdcard, DummyTimesource);

    let mut i2c = I2c::new(p.I2C1, p.PB6, p.PB7, Irqs, p.GPDMA1_CH1, p.GPDMA1_CH2, Default::default());
    let motor = Output::new(p.PB10, Level::Low, Speed::High);
    _spawner.spawn(haptic_task(motor)).unwrap();
    let button = embassy_stm32::gpio::Input::new(p.PB4, embassy_stm32::gpio::Pull::Up);

    i2c.write(0x5A, &[0x80, 0x63]).await.unwrap();
    Timer::after_millis(10).await;
    for i in 0..8u8 {
        i2c.write(0x5A, &[0x41 + i * 2, 0x06]).await.unwrap();
        i2c.write(0x5A, &[0x42 + i * 2, 0x03]).await.unwrap();
    }
    i2c.write(0x5A, &[0x5C, 0x10]).await.unwrap();
    i2c.write(0x5A, &[0x5D, 0x20]).await.unwrap();
    i2c.write(0x5A, &[0x5E, 0x08]).await.unwrap();

    let volume = volume_mgr.open_volume(embedded_sdmmc::VolumeIdx(0)).unwrap();
    let root_dir = volume_mgr.open_root_dir(volume).unwrap();
    let mut file_names: Vec<ShortFileName, MAX_FILES> = Vec::new();
    
    volume_mgr.iterate_dir(root_dir, |entry| {
        if !entry.attributes.is_directory() && !entry.attributes.is_hidden() {
            let ext = entry.name.extension();
            if ext == b"WAV" || ext == b"wav" { 
                if file_names.len() < MAX_FILES {
                    let idx = file_names.len();
                    let _ = file_names.push(entry.name.clone()); 
                    unsafe { FILE_SIZES[idx] = entry.size; }
                }
            }
        }
    }).unwrap();

    let mut selected: usize = 0;
    let mut current_idx: usize = 0;
    let mut app_state = AppState::Menu;
    let mut last_wheel_pos: Option<i8> = None; 
    let mut player_cursor: usize = 1; 
    let mut current_file: Option<embedded_sdmmc::File> = None;
    let mut last_drawn_seconds: u32 = 9999;

    draw_menu(&mut display, &file_names, selected);

    loop {
        if let Some(ref file) = current_file {
            if !BUF_READY.load(Ordering::Relaxed) && AUDIO_PLAYING.load(Ordering::Relaxed) {
                let active_a = USE_BUF_A.load(Ordering::Relaxed);
                let n = unsafe {
                    if active_a { volume_mgr.read(*file, &mut *core::ptr::addr_of_mut!(BUF_B)).unwrap_or(0) } 
                    else { volume_mgr.read(*file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap_or(0) }
                };
                NEXT_LEN.store(n as u32, Ordering::Relaxed);
                PLAYED_BYTES.store(PLAYED_BYTES.load(Ordering::Relaxed) + n as u32, Ordering::Relaxed);
                BUF_READY.store(true, Ordering::Relaxed);
            }

            if FILE_DONE.load(Ordering::Relaxed) {
                FILE_DONE.store(false, Ordering::Relaxed);
                current_idx = (current_idx + 1) % file_names.len();
                if let Some(f) = current_file { volume_mgr.close_file(f).ok(); }
                
                let target_base = file_names[current_idx].base_name();
                unsafe { CURRENT_BMP_LEN = 0; }
                
                let mut found_bmp: Option<ShortFileName> = None;
                volume_mgr.iterate_dir(root_dir, |entry| {
                    if entry.name.base_name() == target_base && entry.name.extension() == b"BMP" { found_bmp = Some(entry.name.clone()); }
                }).unwrap();

                if let Some(bmp_name) = found_bmp {
                    if let Ok(f) = volume_mgr.open_file_in_dir(root_dir, &bmp_name, embedded_sdmmc::Mode::ReadOnly) {
                        unsafe { CURRENT_BMP_LEN = volume_mgr.read(f, &mut *core::ptr::addr_of_mut!(IMAGE_BUF)).unwrap_or(0); }
                        volume_mgr.close_file(f).ok();
                    }
                }

                let file = volume_mgr.open_file_in_dir(root_dir, &file_names[current_idx], embedded_sdmmc::Mode::ReadOnly).unwrap();
                let mut header = [0u8; 44];
                volume_mgr.read(file, &mut header).unwrap();
                let n = unsafe { volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap() };
                current_file = Some(file);
                
                PLAY_LEN.store(n as u32, Ordering::Relaxed);
                PLAYED_BYTES.store(n as u32, Ordering::Relaxed);
                
                USE_BUF_A.store(true, Ordering::Relaxed);
                PLAY_IDX.store(0, Ordering::Relaxed);
                BUF_READY.store(false, Ordering::Relaxed);
                AUDIO_PLAYING.store(true, Ordering::Relaxed);
                
                if app_state == AppState::Playing {
                    draw_player(&mut display, &file_names[current_idx], player_cursor, true, unsafe { &IMAGE_BUF[..CURRENT_BMP_LEN] });
                    last_drawn_seconds = 9999;
                }
            }
        }

        if app_state == AppState::Playing && AUDIO_PLAYING.load(Ordering::Relaxed) {
            let played = PLAYED_BYTES.load(Ordering::Relaxed);
            let current_seconds = played / 44100;
            if current_seconds != last_drawn_seconds {
                last_drawn_seconds = current_seconds;
                let total = unsafe { FILE_SIZES[current_idx] };
                update_progress_bar(&mut display, played, total);
            }
        }

        let mut buf = [0u8; 2];
        if i2c.write_read(0x5A, &[0x00], &mut buf).await.is_ok() {
            let touched = (buf[0] as u16) | ((buf[1] as u16) << 8);
            let wheel_touch = touched & 0xFF;
            let mut current_pos: Option<i8> = None;
            for i in 0..8 { if wheel_touch & (1 << i) != 0 { current_pos = Some(i as i8); break; } }

            if let Some(c_pos) = current_pos {
                if let Some(l_pos) = last_wheel_pos {
                    let diff = (c_pos - l_pos + 8) % 8;
                    let mut move_down = false;
                    let mut move_up = false;
                    if diff == 1 || diff == 2 { move_down = true; } 
                    else if diff == 7 || diff == 6 { move_up = true; }

                    if move_up {
                        HAPTIC_TRIGGER.store(true, Ordering::Relaxed);
                        match app_state {
                            AppState::Menu => { if selected > 0 { let old = selected; selected -= 1; update_menu_selection(&mut display, &file_names, old, selected); } }
                            AppState::Playing => { if player_cursor > 0 { let old = player_cursor; player_cursor -= 1; update_player_cursor(&mut display, old, player_cursor); } }
                        }
                    } else if move_down {
                        HAPTIC_TRIGGER.store(true, Ordering::Relaxed);
                        match app_state {
                            AppState::Menu => { if selected < file_names.len().saturating_sub(1) { let old = selected; selected += 1; update_menu_selection(&mut display, &file_names, old, selected); } }
                            AppState::Playing => { if player_cursor < 3 { let old = player_cursor; player_cursor += 1; update_player_cursor(&mut display, old, player_cursor); } }
                        }
                    }
                }
                last_wheel_pos = Some(c_pos);
            } else { last_wheel_pos = None; }
        }

        if button.is_low() {
            Timer::after_millis(30).await;
            if button.is_low() {
                match app_state {
                    AppState::Menu => {
                        if !file_names.is_empty() {
                            AUDIO_PLAYING.store(false, Ordering::Relaxed);
                            if let Some(f) = current_file { volume_mgr.close_file(f).ok(); }
                            
                            let target_base = file_names[selected].base_name();
                            unsafe { CURRENT_BMP_LEN = 0; }
                            
                            let mut found_bmp: Option<ShortFileName> = None;
                            volume_mgr.iterate_dir(root_dir, |entry| {
                                if entry.name.base_name() == target_base && entry.name.extension() == b"BMP" { found_bmp = Some(entry.name.clone()); }
                            }).unwrap();

                            if let Some(bmp_name) = found_bmp {
                                if let Ok(f) = volume_mgr.open_file_in_dir(root_dir, &bmp_name, embedded_sdmmc::Mode::ReadOnly) {
                                    unsafe { CURRENT_BMP_LEN = volume_mgr.read(f, &mut *core::ptr::addr_of_mut!(IMAGE_BUF)).unwrap_or(0); }
                                    volume_mgr.close_file(f).ok();
                                }
                            }

                            let file = volume_mgr.open_file_in_dir(root_dir, &file_names[selected], embedded_sdmmc::Mode::ReadOnly).unwrap();
                            let mut header = [0u8; 44];
                            volume_mgr.read(file, &mut header).unwrap();
                            let n = unsafe { volume_mgr.read(file, &mut *core::ptr::addr_of_mut!(BUF_A)).unwrap() };
                            current_file = Some(file);
                            current_idx = selected;
                            
                            PLAY_LEN.store(n as u32, Ordering::Relaxed);
                            PLAYED_BYTES.store(n as u32, Ordering::Relaxed);
                            
                            USE_BUF_A.store(true, Ordering::Relaxed);
                            PLAY_IDX.store(0, Ordering::Relaxed);
                            BUF_READY.store(false, Ordering::Relaxed);
                            FILE_DONE.store(false, Ordering::Relaxed);
                            AUDIO_PLAYING.store(true, Ordering::Relaxed);
                            
                            app_state = AppState::Playing;
                            player_cursor = 1; 
                            draw_player(&mut display, &file_names[selected], player_cursor, true, unsafe { &IMAGE_BUF[..CURRENT_BMP_LEN] });
                            last_drawn_seconds = 9999;
                        }
                    }
                    AppState::Playing => {
                        match player_cursor {
                            0 => {
                                let len = file_names.len();
                                if len > 0 { current_idx = (current_idx + len.saturating_sub(2)) % len; FILE_DONE.store(true, Ordering::Relaxed); }
                            }
                            1 => {
                                let playing = AUDIO_PLAYING.load(Ordering::Relaxed);
                                AUDIO_PLAYING.store(!playing, Ordering::Relaxed);
                                update_player_status(&mut display, !playing);
                            }
                            2 => { FILE_DONE.store(true, Ordering::Relaxed); }
                            3 => {
                                app_state = AppState::Menu;
                                selected = current_idx;
                                draw_menu(&mut display, &file_names, selected);
                            }
                            _ => {}
                        }
                    }
                }
                while button.is_low() { Timer::after_millis(10).await; }
            }
        }
        Timer::after_millis(10).await;
    }
}

// ==== UI FUNCTIONS ====

fn draw_menu<D: DrawTarget<Color = Rgb565>>(display: &mut D, files: &Vec<ShortFileName, MAX_FILES>, selected: usize) {
    display.clear(Rgb565::BLACK).ok();
    let title_style = MonoTextStyle::new(&FONT_9X15_BOLD, Rgb565::CYAN);
    Text::new("NucleoPod", Point::new(115, 20), title_style).draw(display).ok();
    for (i, name) in files.iter().enumerate() {
        let y = 50 + i as i32 * 25;
        let is_selected = i == selected;
        if is_selected { Rectangle::new(Point::new(5, y - 15), Size::new(310, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED)).draw(display).ok(); }
        let color = if is_selected { Rgb565::WHITE } else { Rgb565::CSS_STEEL_BLUE };
        let name_str = core::str::from_utf8(name.base_name()).unwrap_or("???");
        let display_str: String<20> = String::try_from(name_str).unwrap_or_default();
        Text::new(&display_str, Point::new(10, y), MonoTextStyle::new(&FONT_9X15, color)).draw(display).ok();
    }
}

fn draw_player<D: DrawTarget<Color = Rgb565>>(display: &mut D, filename: &ShortFileName, cursor: usize, is_playing: bool, bmp_data: &[u8]) {
    display.clear(Rgb565::BLACK).ok();
    Text::new("Now Playing", Point::new(70, 20), MonoTextStyle::new(&FONT_9X15_BOLD, Rgb565::CSS_GOLD)).draw(display).ok();
    
    let mut cover_drawn = false;
    if !bmp_data.is_empty() {
        if let Ok(bmp) = Bmp::from_slice(bmp_data) {
            Image::new(&bmp, Point::new(20, 45)).draw(display).ok();
            cover_drawn = true;
        }
    }
    
    if !cover_drawn {
        Rectangle::new(Point::new(20, 45), Size::new(120, 120))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_STEEL_BLUE, 2)).draw(display).ok();
        Text::new("No Cover", Point::new(45, 105), MonoTextStyle::new(&FONT_9X15, Rgb565::CSS_STEEL_BLUE)).draw(display).ok();
    }

    let name_str = core::str::from_utf8(filename.base_name()).unwrap_or("???");
    let display_str: String<20> = String::try_from(name_str).unwrap_or_default();
    Text::new(&display_str, Point::new(145, 80), MonoTextStyle::new(&FONT_9X15, Rgb565::WHITE)).draw(display).ok();
    
    let labels = ["[ << ]", "[ || ]", "[ >> ]"];
    let x_positions = [19i32, 93, 167]; 

    for (i, (label, x)) in labels.iter().zip(x_positions.iter()).enumerate() {
        if i == cursor {
            Rectangle::new(Point::new(*x - 3, 220), Size::new(60, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED)).draw(display).ok();
        }
        let color = if i == cursor { Rgb565::WHITE } else { Rgb565::CSS_STEEL_BLUE };
        Text::new(label, Point::new(*x, 235), MonoTextStyle::new(&FONT_9X15, color)).draw(display).ok();
    }

    if cursor == 3 {
        Rectangle::new(Point::new(81, 260), Size::new(78, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED)).draw(display).ok();
    }
    let back_color = if cursor == 3 { Rgb565::WHITE } else { Rgb565::CSS_STEEL_BLUE };
    Text::new("[ BACK ]", Point::new(84, 275), MonoTextStyle::new(&FONT_9X15, back_color)).draw(display).ok();

    update_player_status(display, is_playing);
}

// LOGICA ANTI-FLICKER PERFECTA
fn update_progress_bar<D: DrawTarget<Color = Rgb565>>(display: &mut D, played_bytes: u32, total_bytes: u32) {
    let total_sec = total_bytes / 44100;
    let played_sec = played_bytes / 44100;
    
    let m_tot = total_sec / 60;
    let s_tot = total_sec % 60;
    let m_play = played_sec / 60;
    let s_play = played_sec % 60;

    // Folosim Builder-ul pentru ca textul sa isi deseneze automat un patrat negru in spate (Flicker-Free text)
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_9X15)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();

    let mut play_str: String<16> = String::new();
    write!(play_str, "{:02}:{:02}", m_play, s_play).ok();
    Text::new(&play_str, Point::new(10, 185), text_style).draw(display).ok();

    let mut tot_str: String<16> = String::new();
    write!(tot_str, "{:02}:{:02}", m_tot, s_tot).ok();
    Text::new(&tot_str, Point::new(185, 185), text_style).draw(display).ok();

    let bar_y = 195;
    let bar_x = 15;
    let bar_w = 210;
    
    let total = total_bytes.max(1);
    let played = played_bytes.min(total);
    let fill_width = ((played as u64 * bar_w as u64) / total as u64) as u32;
    
    let old_width = unsafe { LAST_FILL_WIDTH };

    // ANTI-FLICKER: Nu mai curatam toata bara decat daca incepe o melodie noua!
    if played == 0 || fill_width < old_width {
        Rectangle::new(Point::new(bar_x - 4, bar_y - 4), Size::new(bar_w as u32 + 8, 10))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK)).draw(display).ok();
    } else if fill_width != old_width {
        // Daca melodia doar merge inainte, stergem CHIRURGICAL doar fosta bulina cu un patratel negru (8x8 px)
        Rectangle::new(Point::new(bar_x + old_width as i32 - 4, bar_y - 3), Size::new(8, 8))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK)).draw(display).ok();
    }
        
    // Desenam linia de fundal (invizibil dpdv flicker pentru ca este doar 2 pixeli inaltime)
    Rectangle::new(Point::new(bar_x, bar_y), Size::new(bar_w as u32, 2))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DIM_GRAY)).draw(display).ok();
    
    if fill_width > 0 {
        Rectangle::new(Point::new(bar_x, bar_y), Size::new(fill_width, 2))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE)).draw(display).ok();
    }
    
    // Desenam bulina noua in fata
    Circle::new(Point::new(bar_x + fill_width as i32 - 4, bar_y - 3), 8)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE)).draw(display).ok();

    // Memoram pozitia curenta pentru secunda urmatoare
    unsafe { LAST_FILL_WIDTH = fill_width; }
}

fn update_player_cursor<D: DrawTarget<Color = Rgb565>>(display: &mut D, old_cursor: usize, new_cursor: usize) {
    let labels = ["[ << ]", "[ || ]", "[ >> ]"];
    let x_positions = [19i32, 93, 167]; 

    if old_cursor < 3 {
        let x = x_positions[old_cursor];
        Rectangle::new(Point::new(x - 3, 220), Size::new(60, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK)).draw(display).ok();
        Text::new(labels[old_cursor], Point::new(x, 235), MonoTextStyle::new(&FONT_9X15, Rgb565::CSS_STEEL_BLUE)).draw(display).ok();
    } else {
        Rectangle::new(Point::new(81, 260), Size::new(78, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK)).draw(display).ok();
        Text::new("[ BACK ]", Point::new(84, 275), MonoTextStyle::new(&FONT_9X15, Rgb565::CSS_STEEL_BLUE)).draw(display).ok();
    }

    if new_cursor < 3 {
        let x = x_positions[new_cursor];
        Rectangle::new(Point::new(x - 3, 220), Size::new(60, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED)).draw(display).ok();
        Text::new(labels[new_cursor], Point::new(x, 235), MonoTextStyle::new(&FONT_9X15, Rgb565::WHITE)).draw(display).ok();
    } else {
        Rectangle::new(Point::new(81, 260), Size::new(78, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED)).draw(display).ok();
        Text::new("[ BACK ]", Point::new(84, 275), MonoTextStyle::new(&FONT_9X15, Rgb565::WHITE)).draw(display).ok();
    }
}

fn update_player_status<D: DrawTarget<Color = Rgb565>>(display: &mut D, is_playing: bool) {
    // AJUSTAT: Folosim builder-ul pentru a desena direct cu background negru (fara flicker)
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_9X15)
        .text_color(Rgb565::CSS_LIME_GREEN)
        .background_color(Rgb565::BLACK)
        .build();
    
    // Spatiile adaugate la final sunt intentionate! Ajuta la acoperirea cu negru a literelor ramase de la cuvantul celalalt
    let status = if is_playing { ">> Playing   " } else { "|| Paused    " };
    
    // AJUSTAT: Am mutat textul mult la dreapta (X=145, fix sub titlul melodiei) si Y=110
    Text::new(status, Point::new(145, 110), style).draw(display).ok();
}

fn update_menu_selection<D: DrawTarget<Color = Rgb565>>(display: &mut D, files: &Vec<ShortFileName, MAX_FILES>, old_idx: usize, new_idx: usize) {
    let y_old = 50 + old_idx as i32 * 25;
    Rectangle::new(Point::new(5, y_old - 15), Size::new(310, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK)).draw(display).ok();
    let name_old = core::str::from_utf8(files[old_idx].base_name()).unwrap_or("???");
    let disp_old: String<20> = String::try_from(name_old).unwrap_or_default();
    Text::new(&disp_old, Point::new(10, y_old), MonoTextStyle::new(&FONT_9X15, Rgb565::CSS_STEEL_BLUE)).draw(display).ok();

    let y_new = 50 + new_idx as i32 * 25;
    Rectangle::new(Point::new(5, y_new - 15), Size::new(310, 22)).into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_INDIAN_RED)).draw(display).ok();
    let name_new = core::str::from_utf8(files[new_idx].base_name()).unwrap_or("???");
    let disp_new: String<20> = String::try_from(name_new).unwrap_or_default();
    Text::new(&disp_new, Point::new(10, y_new), MonoTextStyle::new(&FONT_9X15, Rgb565::WHITE)).draw(display).ok();
}

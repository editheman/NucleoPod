#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embedded_hal::Pwm;
use embassy_executor::Spawner;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_stm32::timer::Channel;
use embassy_stm32::time::Hertz;
use embassy_stm32::gpio::OutputType;
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    info!("PWM audio test pornit");

    let ch1 = PwmPin::new(p.PA0, OutputType::PushPull);
    let mut pwm = SimplePwm::new(
        p.TIM2,
        Some(ch1),
        None,
        None,
        None,
        Hertz(440),
        Default::default(),
    );

    let max = pwm.get_max_duty();
    pwm.set_duty(Channel::Ch1, max / 2);
    pwm.enable(Channel::Ch1);

    info!("PWM 440Hz pornit pe A0");

    loop {
        Timer::after_secs(1).await;
    }
}
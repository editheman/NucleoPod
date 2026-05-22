#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_stm32::gpio::OutputType;
use embassy_stm32::time::hz;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_stm32::timer::Ch3;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

// (frequency_hz, duration_ms)
// Short "Jingle Bells" phrase
const SONG: &[(u32, u64)] = &[
    (659, 250), (659, 250), (659, 500),
    (659, 250), (659, 250), (659, 500),
    (659, 250), (784, 250), (523, 250), (587, 250), (659, 700),
    (0,   300), // rest
    (698, 250), (698, 250), (698, 250), (698, 250),
    (698, 250), (659, 250), (659, 250), (659, 250),
    (659, 250), (587, 250), (587, 250), (659, 250), (587, 500), (784, 500),
];

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let p = embassy_stm32::init(Default::default());

    // Example wiring: buzzer signal -> D6 (PB10 = TIM2_CH3)
    let buzzer_pin: PwmPin<'_, _, Ch3> = PwmPin::new(p.PB10, OutputType::PushPull);

    let mut pwm = SimplePwm::new(
        p.TIM2,
        None,              // CH1
        None,              // CH2
        Some(buzzer_pin),  // CH3
        None,              // CH4
        hz(440),           // initial frequency
        Default::default(),
    );

    let mut ch = pwm.ch3();
    ch.enable();
    ch.set_duty_cycle_percent(40); // loudness

    loop {
        for &(freq, dur_ms) in SONG {
            if freq == 0 {
                {
                    let mut ch = pwm.ch3();
                    ch.disable();
                }

                Timer::after_millis(dur_ms).await;

                {
                    let mut ch = pwm.ch3();
                    ch.enable();
                    ch.set_duty_cycle_percent(40);
                }
            } else {
                pwm.set_frequency(hz(freq));

                {
                    let mut ch = pwm.ch3();
                    ch.enable();
                    ch.set_duty_cycle_percent(40);
                }

                Timer::after_millis(dur_ms).await;

                {
                    let mut ch = pwm.ch3();
                    ch.disable();
                }

                Timer::after_millis(25).await;
            }
        }

        Timer::after_secs(2).await;
    }
}
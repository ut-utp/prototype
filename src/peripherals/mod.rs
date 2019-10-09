//! Peripherals! The [`Peripherals` supertrait](trait.Peripherals.html) and the
//! rest of the peripheral and device traits.

pub mod gpio;
pub mod adc;
pub mod pwm;
pub mod timers;
pub mod clock;

pub mod input;
pub mod output;

use gpio::Gpio;
use adc::Adc;
use pwm::Pwm;
use timers::Timers;
use clock::Clock;

use input::Input;
use output::Output;

pub trait Peripherals: Gpio + Adc + Pwm + Timers + Clock + Input + Output {
    fn init() -> Self;
}

//! Shims for the various [peripheral traits](crate::peripherals).

// Peripherals:
mod adc;
mod clock;
mod gpio;
mod pwm;
mod timers;

// Devices:
mod input;
mod output;

use lc3_traits::peripherals::{PeripheralSet, Peripherals};

pub use clock::ClockShim;
pub use gpio::GpioShim;
pub use pwm::PwmShim;
pub use timers::TimersShim;

pub type PeripheralsShim<'s> =
    PeripheralSet<'s, GpioShim<'s>, adc::Shim<'s>, pwm::PwmShim, TimersShim<'s>, ClockShim, input::Shim, output::Shim>;

// impl Peripherals for PeripheralsShim {
//     fn init() -> Self {

//     }
// }

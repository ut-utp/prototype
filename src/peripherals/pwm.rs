//! [`Pwm` trait](trait.Pwm.html) and helpers.

// TODO: Switch to enum for pins
// TODO: Add Errors
const NUM_PWM_PINS: u8 = 2; // P0 - P1
pub enum PwmState {
    Enabled,
    Disabled,
}
pub trait Pwm {
    // enable, disable, set duty cycle, enable hystersis. start
    fn set_state(&mut self, pin: u8, state: PwmState) -> Result<(), ()>;
    fn get_state(&self, pin: u8) -> Option<PwmState>;
    // fn get_states() // TODO

    fn set_duty_cycle(duty: u16);
    //Optionally enable hysterisis ?
    fn start(&mut self, pin: u8); //Start the periodic timer interrupt
    fn disable(&mut self, pin: u8);
}
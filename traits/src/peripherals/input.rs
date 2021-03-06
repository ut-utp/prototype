//! [`Input` device trait](Input) and related things.
use crate::peripheral_trait;

use core::sync::atomic::AtomicBool;
use core::fmt::{self, Display};

use serde::{Deserialize, Serialize};

peripheral_trait! {input,
pub trait Input<'a>: Default {
    // Warning! This is stateful!! It marks the current data as read.
    //
    // Also note: this is technically infallible (it's up to the
    // interpreter what to do for some of the edge cases, but
    // we'll presumably just return some default value) but since
    // we're letting the interpreter decide we *do* return a Result
    // type here.
    //
    // Must use interior mutability.
    fn read_data(&self) -> Result<u8, InputError>;
    fn current_data_unread(&self) -> bool;

    fn register_interrupt_flag(&mut self, flag: &'a AtomicBool);
    fn interrupt_occurred(&self) -> bool;
    fn reset_interrupt_flag(&mut self,);

    fn set_interrupt_enable_bit(&mut self, bit: bool);
    fn interrupts_enabled(&self) -> bool;
}}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputError {
    NonUnicodeCharacter(u8),
    IoError,
    NoDataAvailable,
}

impl Display for InputError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use InputError::*;

        match self {
            NonUnicodeCharacter(c) => write!(fmt, "Tried to read a non-unicode input: {:#2X}", c),
            IoError => write!(fmt, "I/O error when reading input"),
            NoDataAvailable => write!(fmt, "Attempted to read when no data had been inputted"),
        }
    }
}

using_std! {
    use std::io::Error;
    impl From<Error> for InputError {
        fn from(_e: Error) -> InputError {
            InputError::IoError
        }
    }
}

// TODO: roll this into the macro
using_std! {
    use std::sync::{Arc, RwLock};
    impl<'a, I: Input<'a>> Input<'a> for Arc<RwLock<I>> {
        fn register_interrupt_flag(&mut self, flag: &'a AtomicBool) {
            RwLock::write(self).unwrap().register_interrupt_flag(flag)
        }

        fn interrupt_occurred(&self) -> bool {
            RwLock::read(self).unwrap().interrupt_occurred()
        }

        fn reset_interrupt_flag(&mut self) {
            RwLock::write(self).unwrap().reset_interrupt_flag()
        }

        fn set_interrupt_enable_bit(&mut self, bit: bool) {
            RwLock::write(self).unwrap().set_interrupt_enable_bit(bit)
        }

        fn interrupts_enabled(&self) -> bool {
            RwLock::read(self).unwrap().interrupts_enabled()
        }

        fn read_data(&self) -> Result<u8, InputError> {
            RwLock::write(self).unwrap().read_data()
        }

        fn current_data_unread(&self) -> bool {
            RwLock::write(self).unwrap().current_data_unread()
        }
    }

    use std::sync::Mutex;
    impl<'a, I: Input<'a>> Input<'a> for Arc<Mutex<I>> {
        fn register_interrupt_flag(&mut self, flag: &'a AtomicBool) {
            Mutex::lock(self).unwrap().register_interrupt_flag(flag)
        }

        fn interrupt_occurred(&self) -> bool {
            Mutex::lock(self).unwrap().interrupt_occurred()
        }

        fn reset_interrupt_flag(&mut self) {
            Mutex::lock(self).unwrap().reset_interrupt_flag()
        }

        fn set_interrupt_enable_bit(&mut self, bit: bool) {
            Mutex::lock(self).unwrap().set_interrupt_enable_bit(bit)
        }

        fn interrupts_enabled(&self) -> bool {
            Mutex::lock(self).unwrap().interrupts_enabled()
        }

        fn read_data(&self) -> Result<u8, InputError> {
            Mutex::lock(self).unwrap().read_data()
        }

        fn current_data_unread(&self) -> bool {
            Mutex::lock(self).unwrap().current_data_unread()
        }
    }
}

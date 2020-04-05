//! Test for the OS provided TRAPs.
//!
//! Assumes that the underlying memory mapped device registers work (they're
//! tested in the `lc3-baseline-sim` crate) and that the peripheral trait impls
//! used also work (the shims from the `lc3-peripheral-shims` crate are used;
//! they also have their own tests).

// // Note: this is brittle and kind of bad; at some point it'd be good to spin off
// // the shared testing infrastructure into a `publish = false` crate that lives
// // in this workspace (TODO, low priority).
// #[path = "../../../../baseline-sim/tests/test_infrastructure/mod.rs"]
// #[macro_use]
// mod test_infrastructure;
// use test_infrastructure::*;

extern crate lc3_test_infrastructure as lti;

use lti::Reg::*;

use lti::assert_eq as eq;

// mod adc;
// mod clock;
mod gpio;
// mod pwm;
// mod timers;

// mod input;
// mod output;

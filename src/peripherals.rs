//! Emulators of various Spectrum's peripheral devices.
pub mod ay;
pub mod joystick;
mod keyboard;
pub mod mouse;
pub mod serial;
pub mod zxprinter;

pub use keyboard::*;

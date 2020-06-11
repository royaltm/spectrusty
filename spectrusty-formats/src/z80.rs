//! **Z80** snapshot format utilities.
//! 
//! ## Implementation specifics
//!
//! When reading:
//!
//! * "Custom" Joystick is always interpreted as Sinclair Left, regardless of key bindings.
//! * Handling of MGT +D, DISCiPLE or Multiface is currently not implemented.
//! * Understands an .xzx extension to the version 3 (additional OUT to port 0x1ffd), but only sends it if
//!   a selected spectrum model would handle it properly.
//!
//! When writing:
//!
//! * ROM is not being saved.
mod common;
mod compress;
mod decompress;
mod loader;
mod saver;

pub use loader::*;
pub use saver::*;

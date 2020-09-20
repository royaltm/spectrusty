/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! **Z80** snapshot format utilities.
//!
//! See the specification reference on [World of Spectrum](https://worldofspectrum.org/faq/reference/z80format.htm).
//!
//! ## Implementation specifics
//!
//! When reading from the **Z80** file:
//!
//! * "Custom" Joystick is always interpreted as Sinclair Left Joystick, regardless of key bindings
//!   that are being ignored at the moment.
//! * Handling of MGT +D, DISCiPLE, or Multiface is currently not implemented.
//! * An `.xzx` extension to version 3 (additional OUT to port 0x1ffd) is being read-only if
//!   a selected spectrum model would handle it properly.
//!
//! When writing to the **Z80** file:
//!
//! * ROMs are not being saved.
mod common;
mod compress;
mod decompress;
mod loader;
mod saver;

pub use loader::*;
pub use saver::*;

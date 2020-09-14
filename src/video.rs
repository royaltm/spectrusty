/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! # Video API
pub mod frame_cache;
mod render_pixels;
mod render_pixels_plus;
pub use spectrusty_core::video::*;
pub use render_pixels::Renderer;
pub use render_pixels_plus::*;

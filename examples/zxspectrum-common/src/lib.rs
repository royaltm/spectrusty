/*
    zxspectrum-common: High-level ZX Spectrum emulator library example.
    Copyright (C) 2020  Rafal Michalski

    zxspectrum-common is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    zxspectrum-common is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
//! High-level ZX Spectrum emulator library example on top of [SPECTRUSTY][spectrusty].
mod config;
#[macro_use] mod models;
mod devices;
mod peripherals;
mod spectrum;
mod video;

pub use config::*;
pub use models::*;
pub use devices::*;
pub use peripherals::*;
pub use spectrum::*;
pub use video::*;

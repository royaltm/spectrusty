/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    SPECTRUSTY is free software: you can redistribute it and/or modify it under
    the terms of the GNU Lesser General Public License (LGPL) as published
    by the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    SPECTRUSTY is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Lesser General Public License for more details.

    You should have received a copy of the GNU Lesser General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
//! Emulator components of various ZX Spectrum peripheral devices for the SPECTRUSTY library.
#[macro_use]
extern crate bitflags;

pub mod ay;
pub mod bus;
pub mod joystick;
pub mod memory;
pub mod mouse;
pub mod network;
pub mod parallel;
pub mod serial;
pub mod storage;
pub mod zxprinter;

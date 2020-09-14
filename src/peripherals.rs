/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Emulator components of various ZX Spectrum peripheral devices.
//!
//! ZX Spectrum keyboard emulation interface.
//!
//! Please see also [this](http://rk.nvg.ntnu.no/sinclair/computers/zxspectrum/spec48versions.htm#issue1)
use bitflags::bitflags;

#[cfg(feature = "peripherals")]
pub use spectrusty_peripherals::*;

bitflags! {
    /// Every key's state is encoded as a single bit on this 40-bit flag type.
    /// * Bit = 1 a key is being pressed.
    /// * Bit = 0 a key is not being pressed.
    #[derive(Default)]
    pub struct ZXKeyboardMap: u64 {
        const V  = 0x00_0000_0001;
        const G  = 0x00_0000_0002;
        const T  = 0x00_0000_0004;
        const N5 = 0x00_0000_0008;
        const N6 = 0x00_0000_0010;
        const Y  = 0x00_0000_0020;
        const H  = 0x00_0000_0040;
        const B  = 0x00_0000_0080;
        const C  = 0x00_0000_0100;
        const F  = 0x00_0000_0200;
        const R  = 0x00_0000_0400;
        const N4 = 0x00_0000_0800;
        const N7 = 0x00_0000_1000;
        const U  = 0x00_0000_2000;
        const J  = 0x00_0000_4000;
        const N  = 0x00_0000_8000;
        const X  = 0x00_0001_0000;
        const D  = 0x00_0002_0000;
        const E  = 0x00_0004_0000;
        const N3 = 0x00_0008_0000;
        const N8 = 0x00_0010_0000;
        const I  = 0x00_0020_0000;
        const K  = 0x00_0040_0000;
        const M  = 0x00_0080_0000;
        const Z  = 0x00_0100_0000;
        const S  = 0x00_0200_0000;
        const W  = 0x00_0400_0000;
        const N2 = 0x00_0800_0000;
        const N9 = 0x00_1000_0000;
        const O  = 0x00_2000_0000;
        const L  = 0x00_4000_0000;
        const SS = 0x00_8000_0000;
        const CS = 0x01_0000_0000;
        const A  = 0x02_0000_0000;
        const Q  = 0x04_0000_0000;
        const N1 = 0x08_0000_0000;
        const N0 = 0x10_0000_0000;
        const P  = 0x20_0000_0000;
        const EN = 0x40_0000_0000;
        const BR = 0x80_0000_0000;
    }
}

/// An interface for providing changes of a **ZX Spectrum** keyboard state to one of the `ULA` chipset emulators.
///
/// This trait is implemented by [ControlUnit][spectrusty_core::chip::ControlUnit] implementations which provide
/// a Spectrum's keyboard interface.
pub trait KeyboardInterface {
    /// Reads the current state of the keyboard.
    fn get_key_state(&self) -> ZXKeyboardMap;
    /// Sets the state of the keyboard.
    fn set_key_state(&mut self, keymap: ZXKeyboardMap);
}

impl ZXKeyboardMap {
    /// Reads the state of 4 key lines from `ZXKeyboardMap` suitable for **ZX Spectrum** internal I/O.
    ///
    ///
    /// ```text
    /// line       b   7f bf df ef f7 fb fd fe
    ///           [4]   B  H  Y  6  5  T  G  V
    ///           [3]   N  J  U  7  4  R  F  C
    ///           [2]   M  K  I  8  3  E  D  X
    ///           [1]  SS  L  O  9  2  W  S  Z
    ///           [0]  BR EN  P  0  1  Q  A CS
    ///
    /// [BR] BREAK/SPACE   [EN] ENTER   [CS] CAPS SHIFT   [SS] SYMBOL SHIFT
    ///
    ///       key bits                           key bits
    /// line  b4,  b3,  b2,  b1,   b0   line  b4,  b3,  b2,   b1,   b0
    /// 0xf7 [5], [4], [3], [2],  [1]   0xef [6], [7], [8],  [9],  [0]
    /// 0xfb [T], [R], [E], [W],  [Q]   0xdf [Y], [U], [I],  [O],  [P]
    /// 0xfd [G], [F], [D], [S],  [A]   0xbf [H], [J], [K],  [L], [EN]
    /// 0xfe [V], [C], [X], [Z], [CS]   0x7f [B], [N], [M], [SS], [BR]
    /// ```
    pub fn read_keyboard(self, line: u8) -> u8 {
        let mask = !line;
        let mut res: u8 = !0;
        if mask == 0 {
            return res;
        }
        let mut keymap = self.bits();
        for _ in 0..5 {
            res = res.rotate_left(1);
            let key = keymap as u8;
            if key & mask != 0 {
                res &= !1; // pressed
            }
            keymap >>= 8;
        }
        // eprintln!("keyscan: {:02x} line: {:02x}", res, line);
        res
    }
    /// Changes the pressed state of the key indicated as a key index.
    pub fn change_key_state(self, key: u8, pressed: bool) -> Self {
        let mask = ZXKeyboardMap::from_bits_truncate(1 << key);
        if pressed {
            self | mask
        }
        else {
            self &!mask
        }
    }
}

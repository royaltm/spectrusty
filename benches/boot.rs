/*
    boot: benchmark program for the SPECTRUSTY library.
    Copyright (C) 2020-2022  Rafal Michalski

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    Author contact information: see Cargo.toml file, section [package.authors].
*/
// cargo +nightly bench --bench boot -- --nocapture
#![feature(test)]
extern crate test;
use test::{black_box, Bencher, stats::Summary};
use spectrusty::chip::{*, ula::*};//, ula128::*, ula3::*};
use spectrusty::memory::*;
use spectrusty::video::VideoFrame;

use rand::prelude::*;
use spectrusty::z80emu::*;

const ROM48: &[u8] = include_bytes!("../resources/roms/48.rom");

const KEYBOARD_INPUT: u16 = 0x10A8;
const NO_DEBUG: Option<CpuDebugFn> = None;

#[bench]
fn bench_boot_spectrum16(ben: &mut Bencher) {
    boot_spectrum::<UlaPAL<Memory16k>, Z80NMOS>(ben, 47, 16104, 16);
}

#[bench]
fn bench_boot_spectrum48(ben: &mut Bencher) {
    boot_spectrum::<UlaPAL<Memory48k>, Z80NMOS>(ben, 86, 40249, 9);
}

fn boot_spectrum<U, C: Cpu>(ben: &mut Bencher, boot_frms: u64, boot_ts: i32, init_ts: i32)
    where U: UlaCommon + Default
{
    let mut cpu = C::default();
    let mut ula = U::default();
    ula.memory_mut().fill_mem(.., random).unwrap();
    ula.memory_mut().load_into_rom(ROM48).unwrap();
    let Summary { median, .. } = ben.bench(|ben| {
        ben.iter(|| {
            ula.reset(&mut cpu, true);
            ula.set_frame_tstate(init_ts);
            for _ in 0..boot_frms {
                let _ = ula.execute_next_frame(&mut cpu);
            }
            while cpu.get_pc() != KEYBOARD_INPUT {
                let _ = ula.execute_single_step(&mut cpu, NO_DEBUG);
            }
            assert_eq!(ula.current_tstate(), boot_ts);
            // println!("{}", ula.current_frame());
            black_box(&mut cpu);
        });
        Ok(())
    }).unwrap().unwrap();
    let time = median / 1.0e9;
    let boot_cycles = boot_frms * U::VideoFrame::FRAME_TSTATES_COUNT as u64 + boot_ts as u64;
    eprintln!("boot cycles: {}", boot_cycles);
    eprintln!("boots / s: {:.0}", 1.0/time);
    eprintln!("cycles / s: {:.0}MHz", (boot_cycles as f64/time) / 1.0e6);
    eprintln!("median time: {} s", time);
}

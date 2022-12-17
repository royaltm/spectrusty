/*
    test_contended_halt: tests for the SPECTRUSTY library.
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
//! Tests the custom emulation of the Z80 halted state with the memory contention.
use spectrusty::z80emu::{*, opconsts::HALT_OPCODE, host::cycles::M1_CYCLE_TS};
use spectrusty::memory::{Memory64k, ZxMemory};
use spectrusty::chip::{*, ula::*, ula128::*, ula3::*, plus::*};
use spectrusty::clock::*;
use spectrusty::video::*;

fn ula_contended_halt<U>(addr: u16, vc: Ts, hc: Ts) -> u8
    where U: UlaCommon + Default + Clone +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs> +
             for<'a> UlaPlusInner<'a>
{
    let mut ula = U::default();
    ula.set_video_ts(VideoTs::new(vc, hc));
    ula.memory_mut().page_mut((addr / 0x4000) as u8).unwrap()[addr as usize % 0x4000] = HALT_OPCODE;
    let mut cpu = Z80NMOS::default();
    cpu.reset();
    cpu.set_pc(addr);
    let mut cpu1 = cpu.clone();
    let mut ula1 = ula.clone();

    assert_eq!(cpu.is_halt(), false);
    ula.execute_next_frame(&mut cpu);
    assert_eq!(cpu.is_halt(), true);

    let tsc1 = execute_next_frame_no_halt_emu(&mut ula1, &mut cpu1, 1);
    // println!("{:?} {:?} fr: {:?}", tsc1.tsc, ula.tsc, ula.frame_tstate());
    assert_eq!(tsc1, ula.current_video_ts());
    assert_eq!(cpu1, cpu);
    cpu.get_r()
    // println!("=================================");
}

fn execute_next_frame_no_halt_emu<U, C>(ula: &mut U, cpu: &mut C, mut halt_limit: u32) -> VideoTs
    where C: Cpu,
          U: UlaCommon +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs>
{
    assert_eq!(cpu.is_halt(), false);
    ula.ensure_next_frame();
    let mut tsc = ula.current_video_clock();
    loop {
        match cpu.execute_with_limit(ula, &mut tsc, U::VideoFrame::VSL_COUNT) {
            Ok(()) => break,
            Err(BreakCause::Halt) => {
                // println!("HALT {:04x} {:?}", cpu.get_pc(), tsc.tsc);
                assert_ne!(halt_limit, 0);
                halt_limit -= 1;
                continue
            },
            Err(_) => unreachable!()
        }
    }
    assert_eq!(cpu.is_halt(), true);
    assert_eq!(halt_limit, 0);
    while tsc.hc < -(M1_CYCLE_TS as Ts) {
        match cpu.execute_next::<_,_,CpuDebugFn>(ula, &mut tsc, None) {
            Ok(()) => (),
            Err(_) => unreachable!()
        }
    }

    tsc.into()
}

fn ula_contended_all_tstates<U>()
    where U: UlaCommon + Default + Clone +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs> +
             for<'a> UlaPlusInner<'a>
{
    let mut sums = [0u32;2];
    let mut test_line = |vc| for hc in U::VideoFrame::HTS_RANGE {
        // println!("  hc: {:?}", hc);
        sums[0] += ula_contended_halt::<U>(0, vc, hc) as u32;
        sums[1] += ula_contended_halt::<U>(0x4000, vc, hc) as u32;
    };

    #[cfg(not(debug_assertions))]
    for vc in 0..=U::VideoFrame::VSL_COUNT {
        // println!("vc: {:?}", vc);
        test_line(vc);
    }
    #[cfg(debug_assertions)]
    for vc in (0..3)
              .chain(U::VideoFrame::VSL_PIXELS.start-3..U::VideoFrame::VSL_PIXELS.start + 4)
              .chain(U::VideoFrame::VSL_PIXELS.end-3..U::VideoFrame::VSL_PIXELS.end + 4)
              .chain(U::VideoFrame::VSL_COUNT-3..=U::VideoFrame::VSL_COUNT) {
        // println!("vc: {:?}", vc);
        test_line(vc);
    }
    assert_ne!(sums[0], sums[1]);
}

#[test]
#[ignore]
fn test_contened_halt() {
    println!("\nUlaPAL");
    ula_contended_all_tstates::<UlaPAL<Memory64k>>();
    println!("UlaNTSC");
    ula_contended_all_tstates::<UlaNTSC<Memory64k>>();
    println!("Ula128");
    ula_contended_all_tstates::<Ula128>();
    println!("Ula3");
    ula_contended_all_tstates::<Ula3>();
}

fn ula_halt_irq<U>(vc: Ts, hc: Ts, late_timings: bool, halt_limit: u32) -> u16
    where U: UlaCommon + Default + Clone +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs> +
             for<'a> UlaPlusInner<'a>
{
    let mut ula = U::default();
    ula.set_late_timings(late_timings);
    ula.set_video_ts(VideoTs::new(vc, hc));
    ula.memory_mut().fill_mem(.., || HALT_OPCODE).unwrap();
    ula.memory_mut().page_mut(0).unwrap()[0x38..0x3B].copy_from_slice(&[
              // +13       IRQ
        0x03, // + 6 0038  INC  BC
        0xFB, // + 4 0039  EI
        0xC9, // +10 003A  RET
    ]);
    let mut cpu = Z80NMOS::default();
    cpu.reset();
    cpu.set_sp(0x0000);
    cpu.set_pc(0x8000);
    cpu.set_reg16(StkReg16::BC, 0);
    cpu.enable_interrupts();
    let mut cpu1 = cpu.clone();
    let mut ula1 = ula.clone();

    assert_eq!(cpu.is_halt(), false);
    ula.execute_next_frame(&mut cpu);
    assert_eq!(cpu.is_halt(), true);

    let tsc1 = execute_next_frame_no_halt_emu(&mut ula1, &mut cpu1, halt_limit);

    assert_eq!(tsc1, ula.current_video_ts());
    assert_eq!(cpu1, cpu);

    cpu.get_reg16(StkReg16::BC)
}

fn ula_halt_irq_tstates<U>(late_timings: bool, hc_stop: i16)
    where U: UlaCommon + Default + Clone +
             Memory<Timestamp=VideoTs> +
             Io<Timestamp=VideoTs> +
             for<'a> UlaPlusInner<'a>
{
    for hc in U::VideoFrame::HTS_RANGE.start..hc_stop {
        // println!("  hc: {}", hc);
        assert_eq!(1, ula_halt_irq::<U>(0, hc, late_timings, 2));
    }
    for hc in hc_stop..U::VideoFrame::HTS_RANGE.end {
        // println!("  hc: {}", hc);
        assert_eq!(0, ula_halt_irq::<U>(0, hc, late_timings, 1));
    }
    for hc in U::VideoFrame::HTS_RANGE {
        // println!("  hc: {}", hc);
        assert_eq!(0, ula_halt_irq::<U>(1, hc, late_timings, 1));
    }
}

#[test]
fn test_ula_halt_irq() {
    ula_halt_irq_tstates::<UlaPAL<Memory64k>>(false, 28);
    ula_halt_irq_tstates::<UlaPAL<Memory64k>>(true, 27);
}

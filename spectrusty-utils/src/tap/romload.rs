/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Tools for instant **TAP** loading and verifying via ROM loading routines.
use std::num::Wrapping;
use std::io::{self, Read};
use spectrusty::z80emu::{Cpu, CpuFlags, Prefix, StkReg16, Reg8};
use spectrusty::memory::ZxMemory;

/// Detects if a loading routine in Spectrum's ROM has been called and if so, attempts to
/// instantly load (or verify) data directly into the emulator's memory, reading from the
/// provided data source.
///
/// Provide a mutable reference to a `cpu` and `memory` instances of your emulator.
///
/// Provide a closure that will be called only if loading or verifying will be performed.
/// The closure should return a source of **TAPE** data as an implementation of a [Read] trait.
///
/// If at least one byte has been read from a data source, the state of the `cpu` will be
/// modified in the following way:
/// * On header flag byte mismatch, data verifying mismatch, or loading finished, the ROM loading
///   routine will resume from address `0x05DB`.
/// * On missing data or reading error, the loading routine will resume from address `0x05CD`
///   with only `Z` and `H` flags set and other flags reset.
///
/// In all the above cases the registers `HL`, `IX`, `DE`, `BC`, `AF`, and `A'F'` will be set
/// accordingly to match the normal loading procedure outcome.
///
/// If detection of the loading routine has been positive but reading from the data stream
/// provided no bytes, the state of the `cpu` will remain unmodified.
///
/// Return values:
/// * `Ok(None)` - No ROM loading or verifying detected, the closure was not called, the state
///   of `cpu` and `memory` have not been altered.
/// * `Ok(Some(0))` - The closure was called but data was empty, the state of `cpu` and `memory`
///   have not been altered.
/// * `Ok(Some(1))` - The closure was called but the header flag byte was mismatched or there
///   was only a total of a single byte read from a stream.
/// * `Ok(Some(n))` - Otherwise `n` is the number of bytes, read from a data stream. Note that
///   the first byte is a flag byte being used to match the requested TAPE block type and
///   the last byte is a validation check-xor-sum.
///
/// # Errors
/// Any error returned from the closure or while reading data from the source will be returned
/// from this method. The state of the `cpu` and `memory` will be modified or not, depending
/// on how many bytes have been read before the reading error occurred. In any case,
/// the emulator's state will be left in a coherent state.
pub fn try_instant_rom_tape_load_or_verify<C: Cpu, M: ZxMemory, R: Read, F: FnOnce() -> io::Result<R>>(
        cpu: &mut C,
        memory: &mut M,
        acquire_reader: F
    ) -> io::Result<Option<u32>>
{
    let (sp, de, head_match, flags) = match is_rom_loading(cpu, memory) {
        Some(res) => res,
        None => return Ok(None)
    };

    let mut chunk_bytes = acquire_reader()?.bytes();
    let head = match chunk_bytes.next() {
        Some(res) => res?,
        None => return Ok(Some(0))
    };

    cpu.set_sp(sp);
    let c = (cpu.get_reg(Reg8::C, None) & 0x7F) ^ 3;
    cpu.set_reg(Reg8::C, None, c);

    if head_match != head {
        head_mismatch_exit(cpu, head);
        return Ok(Some(1))
    }

    cpu.ex_af_af();
    cpu.set_acc(c);
    cpu.set_flags(flags|CpuFlags::Z);
    cpu.ex_af_af();

    let is_load = flags.cf();
    let mut checksum = head;
    let mut limit = de;
    let mut tgt_addr = Wrapping(cpu.get_index16(Prefix::Xdd));

    for res in chunk_bytes {
        let octet = match res {
            Ok(o) => o,
            Err(e) => {
                tape_timeout_exit(cpu, checksum, tgt_addr.0, limit);
                return Err(e)
            }
        };
        checksum ^= octet;

        if limit == 0 || (!is_load && memory.read(tgt_addr.0) != octet) {
            finished_exit(cpu, checksum, octet, tgt_addr.0, limit);
            return Ok(Some((de - limit) as u32 + 2));
        }

        if is_load {
            memory.write(tgt_addr.0, octet);
        }

        tgt_addr += Wrapping(1);
        limit -= 1;
    }

    tape_timeout_exit(cpu, checksum, tgt_addr.0, limit);
    Ok(Some((de - limit) as u32 + 2))
}

/*
    tape too short
    PC: 0x05CD
    IX: IX + tape data length
    DE: DE - tape data length
    L: 0x01
    H: total xor sum
    B: 0
    C: (C ^ 3) & 0x7F
    A': C
    F': CF=CF, ZF=1
    A: 0
    F: CF=0, ZF=1 (AND A)
*/
#[inline(never)]
fn tape_timeout_exit<C: Cpu>(cpu: &mut C, checksum: u8, tgt_addr: u16, bytes_left: u16) {
    cpu.set_pc(0x05CD);
    cpu.set_index16(Prefix::Xdd, tgt_addr);
    cpu.set_reg16(StkReg16::DE, bytes_left);
    cpu.set_reg2(StkReg16::HL, checksum, 1);
    cpu.set_reg(Reg8::B, None, 0);
    cpu.set_acc(0);
    cpu.set_flags(CpuFlags::Z|CpuFlags::H);
}

/*
    verify failed
    PC: 0x05DB
    IX: IX + offset to unmatched byte
    DE: DE - offset to unmatched byte
    L: unmatched byte
    H: total xor sum
    B: 0xB0
    C: (C ^ 3) & 0x7F
    A': C
    F': CF=0, ZF=1

    load/verify finished
    PC: 0x05DB
    IX: IX + DE
    DE: 0
    L: last byte (checksum)
    H: total xor sum (should be 0 for positive result)
    B: 0xB0
    C: (C ^ 3) & 0x7F
    A': C
    F': CF=CF, ZF=1
*/
fn finished_exit<C: Cpu>(cpu: &mut C, checksum: u8, octet: u8, tgt_addr: u16, bytes_left: u16) {
    cpu.set_pc(0x05DB);
    cpu.set_index16(Prefix::Xdd, tgt_addr);
    cpu.set_reg16(StkReg16::DE, bytes_left);
    cpu.set_reg2(StkReg16::HL, checksum, octet);
    cpu.set_reg(Reg8::B, None, 0xB0);
}

/*
    flag exit (first byte != header match)
    PC: 0x05DB
    IX: IX
    DE: DE
    L: header byte
    H: header byte
    B: 0xB0
    C: C ^ 3
    A': A'
    F': F'
*/
fn head_mismatch_exit<C: Cpu>(cpu: &mut C, head: u8) {
    cpu.set_pc(0x05DB);
    cpu.set_reg2(StkReg16::HL, head, head);
    cpu.set_reg(Reg8::B, None, 0xB0);
}

/*
    DI
    PC: 0x056B..0x0571, (SP): 0x053F
    PC: 0x05E7..0x05FA, (SP): 0x056F, (SP + 2): 0x053F
    IX: destination address
    DE: 0 < data length < 0xFF00
    A' - header match byte
    F' - CF/ZF, CF=1 load, CF=0 verify, ZF=0
*/
/// Returns Some((sp, de, head_match, flags))
fn is_rom_loading<C: Cpu, M: ZxMemory>(cpu: &C, mem: &M) -> Option<(u16, u16, u8, CpuFlags)> {
    if cpu.get_iffs() != (false, false) {
        return None;
    }

    let stack: &[u16] = match cpu.get_pc() {
        0x056B..=0x0570 => &[0x053F],
        0x05E7..=0x05F9 => &[0x056F, 0x053F],
        _ => &[]
    };
    let mut stack = stack.iter();

    let mut cmp16 = match stack.next() {
        Some(&m) => m,
        None => return None,
    };

    let mut sp = cpu.get_sp();
    loop {
        if mem.read16(sp) != cmp16 {
            return None;
        }
        cmp16 = match stack.next() {
            Some(&m) => m,
            None => break,
        };
        sp = sp.wrapping_add(2);
    }

    let de = cpu.get_reg16(StkReg16::DE);
    if !(1..=0xFEFF).contains(&de) {
        return None;
    }

    let (head_match, flags) = cpu.get_alt_reg2(StkReg16::AF);
    let flags = CpuFlags::from_bits_truncate(flags);
    if flags.zf() {
        return None;
    }

    Some((sp, de, head_match, flags))
}

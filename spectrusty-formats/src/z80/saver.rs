/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::convert::TryInto;
use core::iter;
use std::io::{self, Write, Result};

use spectrusty_core::z80emu::{Cpu, StkReg16, Prefix, Z80NMOS};
use spectrusty_core::video::BorderColor;
use spectrusty_core::chip::ReadEarMode;

use crate::snapshot::*;
use crate::StructWrite;
use super::common::*;
use super::compress::*;

fn init_z80_header<C: Cpu>(
        header: &mut Header,
        version: Z80Version,
        cpu: &C,
        border: BorderColor,
        issue: ReadEarMode,
        joystick: Option<JoystickModel>,
        result: &mut SnapshotResult
    )
{
    let r = cpu.get_r();
    let flags1 = if version == Z80Version::V1 {
        Flags1::MEM_COMPRESSED
    }
    else {
        Flags1::empty()
    }
    .with_border_color(border)
    .with_refresh_high_bit(r);
    // TODO: flags1.set(Flags1::BASIC_SAMROM, samrom_basic_switched_in);
    let mut flags2 = Flags2::empty()
    .with_interrupt_mode(cpu.get_im())
    .with_issue2_emulation(issue);
    if !joystick.map(|joy| flags2.insert_joystick_model(joy)).unwrap_or(false) {
        result.insert(SnapshotResult::JOYSTICK_NSUP);
    }

    let (a, f) = cpu.get_reg2(StkReg16::AF);
    header.a = a;
    header.f = f;
    header.bc = cpu.get_reg16(StkReg16::BC).to_le_bytes();
    header.hl = cpu.get_reg16(StkReg16::HL).to_le_bytes();
    header.pc = if version == Z80Version::V1 { cpu.get_pc() } else { 0 }.to_le_bytes();
    header.sp = cpu.get_sp().to_le_bytes();
    header.i = cpu.get_i();
    header.r7 = r & ((!0) >> 1);
    header.flags1 = flags1.bits();
    header.de = cpu.get_reg16(StkReg16::DE).to_le_bytes();
    header.bc_alt = cpu.get_alt_reg16(StkReg16::BC).to_le_bytes();
    header.de_alt = cpu.get_alt_reg16(StkReg16::DE).to_le_bytes();
    header.hl_alt = cpu.get_alt_reg16(StkReg16::HL).to_le_bytes();
    let (a_alt, f_alt) = cpu.get_alt_reg2(StkReg16::AF);
    header.a_alt = a_alt;
    header.f_alt = f_alt;
    header.iy = cpu.get_index16(Prefix::Yfd).to_le_bytes();
    header.ix = cpu.get_index16(Prefix::Xdd).to_le_bytes();
    let (iff1, iff2) = cpu.get_iffs();
    header.iff1 = if iff1 { !0 } else { 0 };
    header.iff2 = if iff2 { !0 } else { 0 };
    header.flags2 = flags2.bits();
}

fn select_hw_model_v2<S: SnapshotCreator>(
        model: ComputerModel,
        ext: Extensions,
        snapshot: &S,
        result: &mut SnapshotResult
    ) -> Result<(u8, bool)>
{
    use ComputerModel::*;
    if ext.intersects(Extensions::PLUS_D) && snapshot.is_plus_d_rom_paged_in()
       || ext.intersects(Extensions::DISCIPLE) && snapshot.is_disciple_rom_paged_in()
       || ext.intersects(Extensions::TR_DOS) && snapshot.is_tr_dos_rom_paged_in()
       || ext.contains(Extensions::IF1|Extensions::SAM_RAM) && snapshot.is_interface1_rom_paged_in()
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a version 2 snapshot with the external ROM paged in"))
    }
    if (ext&!(Extensions::IF1|Extensions::SAM_RAM)) != Extensions::NONE
       || ext.contains(Extensions::IF1|Extensions::SAM_RAM)
    {
        result.insert(SnapshotResult::EXTENSTION_NSUP);
    }
    Ok(match model {
        Spectrum16 if ext.intersects(Extensions::SAM_RAM) => (2, true),
        Spectrum16 if ext.intersects(Extensions::IF1) => (1, true),
        Spectrum16 => (0, true),
        Spectrum48 if ext.intersects(Extensions::SAM_RAM) => (2, false),
        Spectrum48 if ext.intersects(Extensions::IF1) => (1, false),
        Spectrum48 => (0, false),
        SpectrumNTSC => {
            result.insert(SnapshotResult::MODEL_NSUP);
            (if ext.intersects(Extensions::SAM_RAM) { 2 }
            else if ext.intersects(Extensions::IF1) { 1 }
            else { 0 }, false)
        }
        Spectrum128 if ext.intersects(Extensions::IF1) => (4, false),
        Spectrum128 => (3, false),
        SpectrumPlus2 if ext.intersects(Extensions::IF1) => (4, true),
        SpectrumPlus2 => (3, true),
        SpectrumPlus2A => (7, true),
        SpectrumPlus3 => (7, false),
        SpectrumPlus3e => {
            result.insert(SnapshotResult::MODEL_NSUP);
            (7, false)
        }
        SpectrumSE => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a snapshot of SpectrumSE"))
        }
        TimexTC2048|TimexTC2068|TimexTS2068 if ext.intersects(Extensions::IF1)
                             && snapshot.is_interface1_rom_paged_in() => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a snapshot of Timex + IF1 with IF1 ROM paged in"))
        }
        TimexTC2048 => (14, false),
        TimexTC2068 => (15, false),
        TimexTS2068 => (128, false),
    })
}

fn select_hw_model_v3<S: SnapshotCreator>(
        model: ComputerModel,
        ext: Extensions,
        snapshot: &S,
        result: &mut SnapshotResult
    ) -> Result<(u8, bool)>
{
    use ComputerModel::*;
    if ext.intersects(Extensions::TR_DOS) && snapshot.is_tr_dos_rom_paged_in()
       || ext.contains(Extensions::IF1|Extensions::SAM_RAM) && snapshot.is_interface1_rom_paged_in()
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a version 3 snapshot with the external ROM paged in"))
    }
    if ext.contains(Extensions::IF1|Extensions::SAM_RAM) {
        result.insert(SnapshotResult::EXTENSTION_NSUP);
    }
    Ok(match model {
        Spectrum16 if ext.intersects(Extensions::SAM_RAM) => (2, true),
        Spectrum16 if ext.intersects(Extensions::IF1) => (1, true),
        Spectrum16 if ext.intersects(Extensions::PLUS_D|Extensions::DISCIPLE) => (3, true),
        Spectrum16 => (0, true),
        Spectrum48 if ext.intersects(Extensions::SAM_RAM) => (2, false),
        Spectrum48 if ext.intersects(Extensions::IF1) => (1, false),
        Spectrum48 if ext.intersects(Extensions::PLUS_D|Extensions::DISCIPLE) => (3, false),
        Spectrum48 => (0, false),
        SpectrumNTSC => {
            result.insert(SnapshotResult::MODEL_NSUP);
            (if ext.intersects(Extensions::SAM_RAM) { 2 }
            else if ext.intersects(Extensions::IF1) { 1 }
            else if ext.intersects(Extensions::PLUS_D|Extensions::DISCIPLE) { 3 }
            else { 0 }, false)
        }
        Spectrum128 if ext.intersects(Extensions::IF1) => (5, false),
        Spectrum128 if ext.intersects(Extensions::PLUS_D|Extensions::DISCIPLE) => (6, false),
        Spectrum128 => (4, false),
        SpectrumPlus2 if ext.intersects(Extensions::IF1) => (5, true),
        SpectrumPlus2 if ext.intersects(Extensions::PLUS_D|Extensions::DISCIPLE) => (6, true),
        SpectrumPlus2 => (4, true),
        SpectrumPlus2A => (7, true),
        SpectrumPlus3 => (7, false),
        SpectrumPlus3e => {
            result.insert(SnapshotResult::MODEL_NSUP);
            (7, false)
        }
        SpectrumSE => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a snapshot of SpectrumSE"))
        }
        TimexTC2048|TimexTC2068|TimexTS2068 if (ext.intersects(Extensions::IF1) && snapshot.is_interface1_rom_paged_in())
                             || (ext.intersects(Extensions::PLUS_D) && snapshot.is_plus_d_rom_paged_in())
                             || (ext.intersects(Extensions::DISCIPLE) && snapshot.is_disciple_rom_paged_in())
                             => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a snapshot of Timex with extension ROM paged in"))
        }
        TimexTC2048 => (14, false),
        TimexTC2068 => (15, false),
        TimexTS2068 => (128, false),
    })
}

fn init_z80_header_ex<S: SnapshotCreator, C: Cpu>(
        head_ex: &mut HeaderEx,
        cpu: C,
        model: ComputerModel,
        ext: Extensions,
        snapshot: &S,
        select_hw_model: fn(
            ComputerModel, Extensions, &S, &mut SnapshotResult
        ) -> Result<(u8, bool)>,
        mut result: &mut SnapshotResult
    ) -> Result<()>
{
    use ComputerModel::*;
    head_ex.pc = cpu.get_pc().to_le_bytes();
    let (hw_mode, alt_hw) = select_hw_model(model, ext, snapshot, &mut result)?;
    let mut flags3 = Flags3::empty();
    flags3.set(Flags3::ALT_HW_MODE, alt_hw);
    head_ex.hw_mode = hw_mode;
    let res = if let Some(res) = snapshot.ay_state(Ay3_891xDevice::Ay128k) {
        if snapshot.ay_state(Ay3_891xDevice::FullerBox).is_some()
           || snapshot.ay_state(Ay3_891xDevice::Melodik).is_some()
        {
            result.insert(SnapshotResult::SOUND_CHIP_NSUP);
        }
        Some(res)
    }
    else if let Some(res) = snapshot.ay_state(Ay3_891xDevice::FullerBox) {
        flags3.insert(Flags3::AY_SOUND_EMU|Flags3::AY_FULLER_BOX);
        Some(res)
    }
    else if let Some(res) = snapshot.ay_state(Ay3_891xDevice::Melodik) {
        flags3.insert(Flags3::AY_SOUND_EMU);
        Some(res)
    }
    else {
        snapshot.ay_state(Ay3_891xDevice::Timex)
    };
    if let Some((ay_sel_reg, ay_regs)) = res {
        head_ex.ay_sel_reg = ay_sel_reg.into();
        head_ex.ay_regs = *ay_regs;
    }

    head_ex.port1 = match model {
        Spectrum128|SpectrumPlus2|SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
        SpectrumSE => snapshot.ula128_flags().bits(),
        TimexTC2048|TimexTC2068|TimexTS2068 => snapshot.timex_memory_banks(),
        _ => 0
    };
    head_ex.ifrom = match model {
        TimexTC2048|TimexTS2068|TimexTC2068 => {
            if ext.intersects(Extensions::IF1|Extensions::PLUS_D|Extensions::DISCIPLE) {
                result.insert(SnapshotResult::EXTENSTION_NSUP);
            }
            snapshot.timex_flags().bits()
        }
        _ if ext.intersects(Extensions::IF1) && snapshot.is_interface1_rom_paged_in() => {
            !0
        }
        _ => 0
    };
    head_ex.flags3 = flags3.bits();
    Ok(())
}

fn save_ram_pages<W: Write, S: SnapshotCreator, I: Iterator<Item=(u8, usize)>>(
        mut wr: W,
        snapshot: &S,
        pages: I
    ) -> Result<()>
{
    let mut buf = Vec::with_capacity(0x1000);
    for (ptype, page) in pages {
        buf.clear();
        let mem_slice = snapshot.memory_ref(MemoryRange::Ram(page * PAGE_SIZE..(page + 1) * PAGE_SIZE))?;
        compress_write_all(mem_slice, &mut buf)?;
        let (mem_head, slice) = match buf.len().try_into() {
            Ok(core::u16::MAX)|Err(..) => {
                (MemoryHeader::new(core::u16::MAX, ptype), mem_slice)
            }
            Ok(length) => (MemoryHeader::new(length, ptype), &buf[..]),
        };
        mem_head.write_struct(wr.by_ref())?;
        wr.write_all(slice)?;
    }
    wr.flush()
}

fn save_all_v2v3<W: Write, S: SnapshotCreator>(
        version: Z80Version,
        snapshot: &S,
        model: ComputerModel,
        header: &Header,
        head_ex: &HeaderEx,
        mut wr: W
    ) -> Result<()>
{
    use ComputerModel::*;
    header.write_struct(wr.by_ref())?;
    let ex_len: u16 = match version {
        Z80Version::V2 => 23,
        Z80Version::V3 if head_ex.port2 != 0 => 55,
        Z80Version::V3 => 54,
        _ => unreachable!()
    };
    wr.write_all(&ex_len.to_le_bytes()[..])?;
    head_ex.write_struct_with_limit(wr.by_ref(), ex_len as usize)?;

    match model {
        Spectrum16 => {
            save_ram_pages(wr, snapshot, iter::once((8, 0)))
        }
        Spectrum48|SpectrumNTSC|TimexTC2048|TimexTS2068|TimexTC2068 => {
            save_ram_pages(wr, snapshot,
                [(8, 0), (4, 1), (5, 2)].iter().copied())
        }
        Spectrum128|SpectrumPlus2|SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e => {
            save_ram_pages(wr, snapshot,
                (0..8).map(|page| (page as u8 + 3, page))
            )
        }
        _ => unreachable!()
    }
}

fn get_nmos_cpu(cpu: CpuModel, result: &mut SnapshotResult) -> Z80NMOS {
    match cpu {
        CpuModel::NMOS(cpu) => cpu.clone(),
        CpuModel::CMOS(cpu) => {
            result.insert(SnapshotResult::CPU_MODEL_NSUP);
            cpu.clone().into_flavour()
        },
        CpuModel::BM1(cpu) => {
            result.insert(SnapshotResult::CPU_MODEL_NSUP);
            cpu.clone().into_flavour()
        }
    }
}

/// Saves a **Z80** file version 1 into `wr` from the provided reference to a `snapshot` struct
/// implementing [SnapshotCreator].
///
/// # Errors
/// This function may return an error from attempts to write the file or if for some reason
/// a snapshot could not be created.
pub fn save_z80v1<C: SnapshotCreator, W: Write>(
        snapshot: &C,
        mut wr: W
    ) -> Result<SnapshotResult>
{
    use ComputerModel::*;
    let mut result = SnapshotResult::OK;

    let model = snapshot.model();
    match model {
        Spectrum48 => {},
        Spectrum16|SpectrumNTSC|TimexTC2048 => {
            result.insert(SnapshotResult::MODEL_NSUP);
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput,
                        "Z80: can't create a version 1 snapshot of this computer model"))
    };
    let extensions = snapshot.extensions();
    if let Err(bad_ext) = model.validate_extensions(extensions) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            format!("Z80: the model {} can't be saved with {}", model, bad_ext)))
    }

    if extensions.intersects(Extensions::IF1) && snapshot.is_interface1_rom_paged_in()
       || extensions.intersects(Extensions::PLUS_D) && snapshot.is_plus_d_rom_paged_in()
       || extensions.intersects(Extensions::DISCIPLE) && snapshot.is_disciple_rom_paged_in()
       || extensions.intersects(Extensions::TR_DOS) && snapshot.is_tr_dos_rom_paged_in()
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Z80: can't create a version 1 snapshot with the external ROM paged in"))
    }
    if extensions != Extensions::NONE {
        result.insert(SnapshotResult::EXTENSTION_NSUP);
    }

    let cpu = get_nmos_cpu(snapshot.cpu(), &mut result);
    let border = snapshot.border_color();
    let issue = snapshot.issue();
    let joystick = snapshot.joystick();

    if snapshot.ay_state(Ay3_891xDevice::Ay128k).is_some()
       || snapshot.ay_state(Ay3_891xDevice::Melodik).is_some()
       || snapshot.ay_state(Ay3_891xDevice::FullerBox).is_some()
       || snapshot.ay_state(Ay3_891xDevice::Timex).is_some()
    {
        result.insert(SnapshotResult::SOUND_CHIP_NSUP);
    }

    let mut header = Header::default();
    init_z80_header(
        &mut header,
        Z80Version::V1,
        &cpu,
        border,
        issue,
        joystick,
        &mut result
    );

    header.write_struct(wr.by_ref())?;
    let is_16k = model == ComputerModel::Spectrum16;
    let ramend = if is_16k { 0x4000 } else { 0xC000 };
    let mem_slice = snapshot.memory_ref(MemoryRange::Ram(0..ramend))?;
    compress_write_all(mem_slice, wr.by_ref())?;
    if is_16k {
        compress_repeat_write_all(!0, 0x8000, wr.by_ref())?;
    }

    wr.write_all(MEMORY_V1_TERM)?;
    wr.flush()?;
    Ok(result)
}

/// Saves a **Z80** file version 2 into `wr` from the provided reference to a `snapshot` struct
/// implementing [SnapshotCreator].
///
/// # Errors
/// This function may return an error from attempts to write the file or if for some reason
/// a snapshot could not be created.
pub fn save_z80v2<C: SnapshotCreator, W: Write>(
        snapshot: &C,
        wr: W
    ) -> Result<SnapshotResult>
{
    let mut result = SnapshotResult::OK;
    let model = snapshot.model();
    let ext = snapshot.extensions();
    let border = snapshot.border_color();
    let issue = snapshot.issue();
    let joystick = snapshot.joystick();
    let cpu = get_nmos_cpu(snapshot.cpu(), &mut result);
    if !is_cpu_safe_for_snapshot(&cpu) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Z80: can't safely snapshot the CPU state"))
    }
    if let Err(bad_ext) = model.validate_extensions(ext) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            format!("Z80: the model {} can't be saved with {}", model, bad_ext)))
    }

    let mut header = Header::default();
    init_z80_header(
        &mut header,
        Z80Version::V2,
        &cpu,
        border,
        issue,
        joystick,
        &mut result
    );

    let mut head_ex = HeaderEx::default();
    init_z80_header_ex(
        &mut head_ex,
        cpu,
        model,
        ext,
        snapshot,
        select_hw_model_v2,
        &mut result
    )?;

    save_all_v2v3(Z80Version::V2, snapshot, model, &header, &head_ex, wr)?;
    Ok(result)
}

/// Saves a **Z80** file version 3 into `wr` from the provided reference to a `snapshot` struct
/// implementing [SnapshotCreator].
///
/// # Errors
/// This function may return an error from attempts to write the file or if for some reason
/// a snapshot could not be created.
pub fn save_z80v3<C: SnapshotCreator, W: Write>(
        snapshot: &C,
        wr: W
    ) -> Result<SnapshotResult>
{
    use ComputerModel::*;
    let mut result = SnapshotResult::OK;
    let model = snapshot.model();
    let ext = snapshot.extensions();
    let border = snapshot.border_color();
    let issue = snapshot.issue();
    let joystick = snapshot.joystick();
    let cpu = get_nmos_cpu(snapshot.cpu(), &mut result);
    if !is_cpu_safe_for_snapshot(&cpu) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Z80: can't safely snapshot the CPU state"))
    }
    if let Err(bad_ext) = model.validate_extensions(ext) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            format!("Z80: the model {} can't be saved with {}", model, bad_ext)))
    }

    let mut header = Header::default();
    init_z80_header(
        &mut header,
        Z80Version::V3,
        &cpu,
        border,
        issue,
        joystick,
        &mut result
    );

    let mut head_ex = HeaderEx::default();
    init_z80_header_ex(
        &mut head_ex,
        cpu,
        model,
        ext,
        snapshot,
        select_hw_model_v3,
        &mut result
    )?;

    let (ts_lo, ts_hi) = cycles_to_z80(snapshot.current_clock(), model);
    head_ex.ts_lo = ts_lo.to_le_bytes();
    head_ex.ts_hi = ts_hi;
    if (ext.intersects(Extensions::PLUS_D) && snapshot.is_plus_d_rom_paged_in())
        || (ext.intersects(Extensions::DISCIPLE) && snapshot.is_disciple_rom_paged_in())
    {
        head_ex.mgt_rom = !0;
    }

    if let Some(JoystickModel::Sinclair2) = joystick {
        head_ex.joy_bindings = [03,01,03,02,03,04,03,08,03,10];
        head_ex.joy_ascii    = [31,00,32,00,33,00,34,00,35,00];
    }

    match model {
        SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e => {
            head_ex.port2 = snapshot.ula3_flags().bits();
        }
        _ => {
            head_ex.fn1 = !0;
            if !(ext.intersects(Extensions::PLUS_D) && snapshot.is_plus_d_rom_paged_in()) {
                head_ex.fn2 = !0;
            }
        }
    }

    if ext.intersects(Extensions::PLUS_D) {
        head_ex.mgt_type = 16;
    }

    // head_ex.flags4 = 0;
    // head_ex.disciple1 = 0;
    // head_ex.disciple2 = 0;

    save_all_v2v3(Z80Version::V3, snapshot, model, &header, &head_ex, wr)?;
    Ok(result)
}

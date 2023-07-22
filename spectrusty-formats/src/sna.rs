/*
    Copyright (C) 2020-2023  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
/*! **SNA** snapshot format utilities.

48k `SNA` file (LSB first):

| offset | size  | description                              |
|--------|-------|------------------------------------------|
|      0 |     1 | register: I                              |
|      1 |     6 | registers: HL', DE', BC'                 |
|      7 |     2 | registers: AF'                           |
|      9 |     6 | registers: HL, DE, BC                    |
|     15 |     4 | registers: IY, IX                        |
|     19 |     1 | interrupt flags: bit 1=IFF1, bit 2=IFF1) |
|     20 |     1 | register: R                              |
|     21 |     4 | registers: AF                            |
|     23 |     4 | register: SP                             |
|     25 |     1 | interrupt mode: 0=IM0, 1=IM1, 2=IM2      |
|     26 |     1 | border color: 0..=7                      |
|     27 | 49152 | bytes: RAM 16384..=65535                 |

Total size: `49179` bytes

128k extended `SNA` file:

| offset | size  | description                                   |
|--------|-------|-----------------------------------------------|
|      0 |    27 | 48k `SNA` header                              |
|     27 | 16384 | bytes: RAM page 1, bank 5                     |
|  16411 | 16384 | bytes: RAM page 2, bank 2                     |
|  32795 | 16384 | bytes: RAM page 3, currently paged bank       |
|  49179 |     2 | register:  PC                                 |
|  49181 |     1 | byte: last OUT to 0x7ffd                      |
|  49182 |     1 | byte: TR-DOS ROM 1=paged, 0=not paged         |
|  49183 |*16384 | bytes: remaining RAM banks in ascending order |

Total size:
* `131103` bytes if RAM page 3 is not one of bank 5 or bank 2
* `147487` bytes if RAM page 3 is bank 5 or bank 2 (included twice)

*/
use core::mem::size_of;
use std::convert::TryInto;
use std::io::{self, ErrorKind, Error, Read, Write, Seek, SeekFrom, Result};

use spectrusty_core::{
    chip::{Ula128MemFlags, ReadEarMode},
    memory::ZxMemory,
    video::BorderColor,
    z80emu::{Cpu, Prefix, StkReg16, CpuFlags, InterruptMode, Z80NMOS}
};

use crate::{StructRead, StructWrite};
use super::snapshot::*;

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[repr(packed)]
struct SnaHeader {
    i: u8,
    hl_alt: [u8;2],
    de_alt: [u8;2],
    bc_alt: [u8;2],
    f_alt: u8,
    a_alt: u8,
    hl: [u8;2],
    de: [u8;2],
    bc: [u8;2],
    iy: [u8;2],
    ix: [u8;2],
    iffs: u8,
    r: u8,
    f: u8,
    a: u8,
    sp: [u8;2],
    im: u8,
    border: u8
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[repr(packed)]
struct SnaHeader128 {
    pc: [u8;2],
    port_data: u8,
    trdos_rom: u8
}

// Structs must be packed and consist of `u8` or/and arrays of `u8` primitives only.
unsafe impl StructRead for SnaHeader {}
unsafe impl StructRead for SnaHeader128 {}
unsafe impl StructWrite for SnaHeader {}
unsafe impl StructWrite for SnaHeader128 {}

/// The length in bytes of the 48k **SNA** file.
pub const SNA_LENGTH: u64 = 49179;

const PAGE_SIZE: usize = 0x4000;

fn read_header<R: Read, C: Cpu>(rd: R, cpu: &mut C) -> Result<BorderColor> {
    let sna = SnaHeader::read_new_struct(rd)?;
    cpu.reset();
    cpu.set_i(sna.i);
    cpu.set_reg16(StkReg16::HL, u16::from_le_bytes(sna.hl_alt));
    cpu.set_reg16(StkReg16::DE, u16::from_le_bytes(sna.de_alt));
    cpu.set_reg16(StkReg16::BC, u16::from_le_bytes(sna.bc_alt));
    cpu.exx();
    cpu.set_acc(sna.a_alt);
    cpu.set_flags(CpuFlags::from_bits_truncate(sna.f_alt));
    cpu.ex_af_af();
    cpu.set_reg16(StkReg16::HL, u16::from_le_bytes(sna.hl));
    cpu.set_reg16(StkReg16::DE, u16::from_le_bytes(sna.de));
    cpu.set_reg16(StkReg16::BC, u16::from_le_bytes(sna.bc));
    cpu.set_index16(Prefix::Yfd, u16::from_le_bytes(sna.iy));
    cpu.set_index16(Prefix::Xdd, u16::from_le_bytes(sna.ix));
    let iff = sna.iffs & (1<<2) != 0;
    cpu.set_iffs(iff, iff);
    cpu.set_r(sna.r);
    cpu.set_acc(sna.a);
    cpu.set_flags(CpuFlags::from_bits_truncate(sna.f));
    cpu.set_sp(u16::from_le_bytes(sna.sp));
    cpu.set_im(sna.im.try_into().map_err(|_| {
       Error::new(ErrorKind::InvalidData, "Not a proper SNA block: invalid interrupt mode")
    })?);
    sna.border.try_into().map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

/// Reads a 48k **SNA** file and inserts its content into provided memory and configures the `Cpu`.
/// Returns a border color on success.
///
/// # Note
/// This function handles only the 48k **SNA** files.
///
/// # Errors
/// This function will return an error if the file is too small.
/// Other errors may also be returned from attempts to read the file.
pub fn read_sna48<R: Read, M: ZxMemory, C: Cpu>(
        mut rd: R,
        cpu: &mut C,
        mem: &mut M
    ) -> Result<BorderColor>
{
    let border = read_header(rd.by_ref(), cpu)?;
    let sp = cpu.get_sp();
    cpu.set_sp(sp.wrapping_add(2));
    cpu.inc_r();
    cpu.inc_r(); // RETN would increase this 2 times
    mem.load_into_mem(0x4000..=0xFFFF, rd).map_err(|_| {
       Error::new(ErrorKind::InvalidData, "SNA: needs at least 48k RAM memory")
    })?;
    cpu.set_pc(mem.read16(sp));
    Ok(border)
}

/// Loads an **SNA** file from `rd` into the provided snapshot `loader` implementing [SnapshotLoader].
///
/// Requires both [Read] and [Seek] implementations to determine the file version.
///
/// # Errors
/// This function will return an error if the file size is incorrect or there is something wrong
/// with the format.
/// Other errors may also be returned from attempts to read the file.
pub fn load_sna<R: Read + Seek, S: SnapshotLoader>(
        mut rd: R,
        loader: &mut S
    ) -> Result<()>
{
    let cur_pos = rd.seek(SeekFrom::Current(0))?;
    let end_pos = rd.seek(SeekFrom::Current(SNA_LENGTH as i64))?;
    if end_pos - cur_pos != SNA_LENGTH {
        return Err(Error::new(ErrorKind::InvalidData, "SNA: wrong size of the supplied stream"));
    }

    let mut sna_ext = SnaHeader128::default();
    let ext_read = sna_ext.read_struct_or_nothing(rd.by_ref())?;

    rd.seek(SeekFrom::Start(cur_pos))?;

    if !ext_read {
        return load_sna48(rd, loader)
    }

    let mut cpu = Z80NMOS::default();
    let border = read_header(rd.by_ref(), &mut cpu)?;
    cpu.set_pc(u16::from_le_bytes(sna_ext.pc));

    let extensions = if sna_ext.trdos_rom != 0 {
        Extensions::TR_DOS
    }
    else {
        Extensions::NONE
    };
    let model = ComputerModel::Spectrum128;
    loader.select_model(model, extensions, border, ReadEarMode::Issue3)
          .map_err(|e| Error::new(ErrorKind::Other, e))?;

    let index48 = [5, 2];
    let last_page = Ula128MemFlags::from_data(sna_ext.port_data)
                    .last_ram_page_bank();
    for page in index48.iter().chain(
                    Some(&last_page).filter(|n| !index48.contains(n))
                ) {
        loader.read_into_memory(
            MemoryRange::Ram(page * PAGE_SIZE..(page + 1) * PAGE_SIZE), rd.by_ref()
        )?;
    }

    rd.seek(SeekFrom::Current(size_of::<SnaHeader128>() as i64))?;

    for page in (0..8).filter(|n| !index48.contains(n) && *n != last_page) {
        loader.read_into_memory(
            MemoryRange::Ram(page * PAGE_SIZE..(page + 1) * PAGE_SIZE), rd.by_ref()
        )?;
    }
    loader.assign_cpu(CpuModel::NMOS(cpu));
    loader.write_port(0x7ffd, sna_ext.port_data);
    if sna_ext.trdos_rom == 1 {
        loader.tr_dos_rom_paged_in();
    }
    Ok(())
}

/// Loads a 48k **SNA** file from `rd` into the provided snapshot `loader` implementing [SnapshotLoader].
///
/// # Note
/// This function handles only the 48k **SNA** files.
///
/// # Errors
/// This function will return an error if the file is too small or the `SP` register points into the ROM page.
/// Other errors may also be returned from attempts to read the file.
pub fn load_sna48<R: Read, S: SnapshotLoader>(
        mut rd: R,
        loader: &mut S
    ) -> Result<()>
{
    let mut cpu = Z80NMOS::default();
    let border = read_header(rd.by_ref(), &mut cpu)?;
    let sp = cpu.get_sp();
    if sp < 0x4000 || sp == 0xFFFF  {
        return Err(Error::new(ErrorKind::InvalidData, "SNA: can't determine the PC address"))
    }
    cpu.set_sp(sp.wrapping_add(2));
    cpu.inc_r();
    cpu.inc_r(); // RETN would increase this 2 times

    let model = ComputerModel::Spectrum48;
    loader.select_model(model, Default::default(), border, ReadEarMode::Issue3)
          .map_err(|e| Error::new(ErrorKind::Other, e))?;

    let pc_offset = sp as usize - 0x4000;
    loader.read_into_memory(MemoryRange::Ram(0..pc_offset), rd.by_ref())?;
    let mut pc = [0u8;2];
    rd.read_exact(&mut pc)?;
    cpu.set_pc(u16::from_le_bytes(pc));
    let rest_offset = pc_offset + 2;
    if rest_offset < 0xC000 {
        loader.read_into_memory(MemoryRange::Ram(rest_offset..0xC000), rd)?;
    }
    loader.assign_cpu(CpuModel::NMOS(cpu));
    Ok(())
}

fn make_header<C: Cpu>(cpu: &C) -> SnaHeader {
    let mut sna = SnaHeader {
        i: cpu.get_i(),
        hl_alt: cpu.get_alt_reg16(StkReg16::HL).to_le_bytes(),
        de_alt: cpu.get_alt_reg16(StkReg16::DE).to_le_bytes(),
        bc_alt: cpu.get_alt_reg16(StkReg16::BC).to_le_bytes(),
        hl: cpu.get_reg16(StkReg16::HL).to_le_bytes(),
        de: cpu.get_reg16(StkReg16::DE).to_le_bytes(),
        bc: cpu.get_reg16(StkReg16::BC).to_le_bytes(),
        iy: cpu.get_index16(Prefix::Yfd).to_le_bytes(),
        ix: cpu.get_index16(Prefix::Xdd).to_le_bytes(),
        r: cpu.get_r(),
        im: match cpu.get_im() {
            InterruptMode::Mode0 => 0,
            InterruptMode::Mode1 => 1,
            InterruptMode::Mode2 => 2,
        },
        sp: cpu.get_sp().to_le_bytes(),
        ..Default::default()
    };
    (sna.a_alt, sna.f_alt) = cpu.get_alt_reg2(StkReg16::AF);
    (sna.a, sna.f) = cpu.get_reg2(StkReg16::AF);
    let (iff1, _) = cpu.get_iffs();
    sna.iffs = (iff1 as u8) << 2;
    sna
}

/// Saves an **SNA** file into `wr` from the provided reference to a `snapshot` struct
/// implementing [SnapshotCreator].
///
/// # Errors
/// This function may return an error from attempts to write the file or if for some reason
/// a snapshot could not be created.
pub fn save_sna<C: SnapshotCreator, W: Write>(
        snapshot: &C,
        mut wr: W
    ) -> Result<SnapshotResult>
{
    use ComputerModel::*;
    let mut result = SnapshotResult::KEYB_ISSUE_NSUP;
    let model = snapshot.model();
    let is_128 = match model {
        Spectrum48 => false,
        Spectrum128 => true,
        SpectrumPlus2|SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|SpectrumSE => {
            result.insert(SnapshotResult::MODEL_NSUP);
            true
        }
        Spectrum16|SpectrumNTSC|TimexTC2048|TimexTC2068|TimexTS2068 => {
            result.insert(SnapshotResult::MODEL_NSUP);
            false
        }
    };

    let extensions = snapshot.extensions();
    if extensions.intersects(Extensions::IF1) && snapshot.is_interface1_rom_paged_in()
       || extensions.intersects(Extensions::PLUS_D) && snapshot.is_plus_d_rom_paged_in()
    {
        return Err(Error::new(ErrorKind::InvalidInput,
                "SNA: can't create a snapshot with the external ROM paged in"))
    }
    if extensions != Extensions::NONE && extensions != Extensions::TR_DOS {
        result.insert(SnapshotResult::EXTENSTION_NSUP);
    }

    let cpu = match snapshot.cpu() {
        CpuModel::NMOS(cpu) => cpu,
        CpuModel::CMOS(cpu) => {
            result.insert(SnapshotResult::CPU_MODEL_NSUP);
            cpu.into_flavour()
        },
        CpuModel::BM1(cpu) => {
            result.insert(SnapshotResult::CPU_MODEL_NSUP);
            cpu.into_flavour()
        }
    };

    if !is_cpu_safe_for_snapshot(&cpu) {
        return Err(Error::new(ErrorKind::InvalidInput, "SNA: can't safely snapshot the CPU state"))
    }

    let mut sna = make_header(&cpu);
    sna.border = snapshot.border_color().into();

    if snapshot.joystick().is_some() {
        result.insert(SnapshotResult::JOYSTICK_NSUP);
    }

    if is_128 || snapshot.ay_state(Ay3_891xDevice::Melodik).is_some()
              || snapshot.ay_state(Ay3_891xDevice::FullerBox).is_some()
              || snapshot.ay_state(Ay3_891xDevice::Timex).is_some() {
        result.insert(SnapshotResult::SOUND_CHIP_NSUP);
    }

    if !is_128 {
        return save_sna48(snapshot, cpu, model == ComputerModel::Spectrum16, sna, wr, result)
    }

    let memflags = snapshot.ula128_flags();
    let mut sna_ext = SnaHeader128 {
        pc: cpu.get_pc().to_le_bytes(),
        port_data: memflags.bits(),
        ..Default::default()
    };

    if extensions.intersects(Extensions::TR_DOS) {
        sna_ext.trdos_rom = snapshot.is_tr_dos_rom_paged_in().into();
    }

    sna.write_struct(wr.by_ref())?;

    let last_page: usize = memflags.last_ram_page_bank();
    let index48 = [5,2,last_page];
    for page in index48.iter() {
        wr.write_all(
            snapshot.memory_ref(MemoryRange::Ram(page * PAGE_SIZE..(page + 1) * PAGE_SIZE))?
        )?;
    }

    sna_ext.write_struct(wr.by_ref())?;

    for page in (0..8).filter(|n| !index48.contains(n) && *n != last_page) {
        wr.write_all(
            snapshot.memory_ref(MemoryRange::Ram(page * PAGE_SIZE..(page + 1) * PAGE_SIZE))?
        )?;
    }

    wr.flush()?;
    Ok(result)
}

fn save_sna48<C: SnapshotCreator, W: Write>(
        snapshot: &C,
        cpu: Z80NMOS,
        is_mem16k: bool,
        mut sna: SnaHeader,
        mut wr: W,
        result: SnapshotResult
    ) -> Result<SnapshotResult>
{
    const ROMSIZE: usize = 0x4000;
    let ramtop = if is_mem16k { 0x7FFF } else { 0xFFFF };
    let sp = cpu.get_sp().wrapping_sub(2);
    if (sp as usize) < ROMSIZE || sp >= ramtop  {
        return Err(Error::new(ErrorKind::InvalidData, "SNA: can't store the PC address"))
    }
    sna.sp = sp.to_le_bytes();
    sna.write_struct(wr.by_ref())?;
    let pc = cpu.get_pc().to_le_bytes();
    let pc_offset = sp as usize - ROMSIZE;
    let mem_slice = snapshot.memory_ref(MemoryRange::Ram(0..pc_offset))?;
    wr.write_all(mem_slice)?;
    wr.write_all(&pc)?;
    let mem_slice = snapshot.memory_ref(MemoryRange::Ram(pc_offset + 2..(ramtop as usize + 1) - ROMSIZE))?;
    wr.write_all(mem_slice)?;
    if is_mem16k {
        io::copy(&mut io::repeat(!0).take(0x8000), &mut wr)?;
    }
    wr.flush()?;
    Ok(result)
}

//! **SNA** snapshot format utilities.
use core::mem::size_of;
use std::convert::TryInto;
use std::io::{ErrorKind, Error, Read, Seek, SeekFrom, Result};

use spectrusty_core::{
    chip::ReadEarMode,
    memory::ZxMemory,
    video::BorderColor,
    z80emu::{Cpu, Prefix, StkReg16, CpuFlags, Z80NMOS}
};

use crate::ReadExactEx;
use super::snapshot::*;

/*
   Offset   Size   Description
   ------------------------------------------------------------------------
   0        1      byte   I
   1        8      word   HL',DE',BC',AF'
   9        10     word   HL,DE,BC,IY,IX
   19       1      byte   Interrupt (bit 2 contains IFF2, 1=EI/0=DI)
   20       1      byte   R
   21       4      words  AF,SP
   25       1      byte   IntMode (0=IM0/1=IM1/2=IM2)
   26       1      byte   BorderColor (0..7, not used by Spectrum 1.7)
   27       49152  bytes  RAM dump 16384..65535
   ------------------------------------------------------------------------
   Total: 49179 bytes

   Offset   Size   Description
   ------------------------------------------------------------------------
   0        27     bytes  SNA header (see above)
   27       16Kb   bytes  RAM bank 5 \
   16411    16Kb   bytes  RAM bank 2  } - as standard 48Kb SNA file
   32795    16Kb   bytes  RAM bank n / (currently paged bank)
   49179    2      word   PC
   49181    1      byte   port 0x7ffd setting
   49182    1      byte   TR-DOS rom paged (1) or not (0)
   49183    16Kb   bytes  remaining RAM banks in ascending order
   ...
   ------------------------------------------------------------------------
   Total: 131103 or 147487 bytes
*/
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

#[repr(packed)]
union SnaHeaderUnion {
    bytes: [u8;size_of::<SnaHeader>()],
    header: SnaHeader
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[repr(packed)]
struct SnaHeader128 {
    pc: [u8;2],
    port_data: u8,
    trdos_rom: u8
}

#[repr(packed)]
union SnaHeader128Union {
    bytes: [u8;size_of::<SnaHeader128>()],
    header: SnaHeader128
}

/// A length in bytes of the 48k **SNA** file.
pub const SNA_LENGTH: u64 = 49179;
pub const PAGE_SIZE: usize = 0x4000;

fn read_header<R: Read, C: Cpu>(mut rd: R, cpu: &mut C) -> Result<BorderColor> {
    let mut sna = SnaHeaderUnion { bytes: Default::default() };
    rd.read_exact(unsafe { &mut sna.bytes })?;
    let sna: SnaHeader = unsafe { sna.header };
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
/// This function handles only the 48k *SNA* file, that is the one with the size of 49179 bytes.
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

/// Loads a **SNA** file into the provided snapshot `loader` from a source.
///
/// Requres both [Read] and [Seek] source to determine file version.
pub fn load_sna<R: Read + Seek, S: SnapshotLoader>(
        mut rd: R,
        loader: &mut S
    ) -> Result<()>
{
    let cur_pos = rd.seek(SeekFrom::Current(0))?;
    let end_pos = rd.seek(SeekFrom::Current(SNA_LENGTH as i64))?;
    let mut sna_ext = SnaHeader128Union { bytes: Default::default() };
    let ext_read = rd.read_exact_or_none(unsafe { &mut sna_ext.bytes })?;
    if end_pos - cur_pos != SNA_LENGTH as u64 {
        return Err(Error::new(ErrorKind::InvalidData, "SNA: wrong size of the supplied stream"));
    }
    rd.seek(SeekFrom::Start(cur_pos))?;
    if !ext_read {
        return load_sna48(rd, loader);
    }

    let sna_ext: SnaHeader128 = unsafe { sna_ext.header };

    let mut cpu = Z80NMOS::default();
    let border = read_header(rd.by_ref(), &mut cpu)?;
    cpu.set_pc(u16::from_le_bytes(sna_ext.pc));

    let extensions = if sna_ext.trdos_rom != 0 {
        Extensions::TR_DOS
    }
    else {
        Extensions::NONE
    };
    let model = ComputerModel::Spectrum128(extensions);
    loader.select_model(model, border, ReadEarMode::Issue3, None)
          .map_err(|e| Error::new(ErrorKind::Other, e))?;

    let index48 = [5, 2];
    let last_page = sna_ext.port_data as usize & 7;
    for page in index48.iter().chain(
                    Some(&last_page).filter(|n| !index48.contains(n))
                ) {
        loader.load_memory_page(
            MemoryRange::Ram(page * PAGE_SIZE..(page + 1) * PAGE_SIZE), rd.by_ref()
        )?;
    }

    rd.seek(SeekFrom::Current(size_of::<SnaHeader128>() as i64))?;

    for page in (0..8).filter(|n| !index48.contains(n) && *n != last_page) {
        loader.load_memory_page(
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

/// Loads a 48k **SNA** file into the provided snapshot `loader` from a source.
///
/// # Note
/// This method assumes the provided stream contains the 48k **SNA** version.
pub fn load_sna48<R: Read, S: SnapshotLoader>(
        mut rd: R,
        loader: &mut S
    ) -> Result<()>
{
    let mut cpu = Z80NMOS::default();
    let border = read_header(rd.by_ref(), &mut cpu)?;
    let sp = cpu.get_sp();
    if sp < 0x4000 && sp == 0xFFFF  {
        return Err(Error::new(ErrorKind::InvalidData, "SNA: can't determine the PC address"))
    }
    cpu.set_sp(sp.wrapping_add(2));
    cpu.inc_r();
    cpu.inc_r(); // RETN would increase this 2 times

    let model = ComputerModel::Spectrum48(Default::default());
    loader.select_model(model, border, ReadEarMode::Issue3, None)
          .map_err(|e| Error::new(ErrorKind::Other, e))?;

    let pc_offset = sp as usize - 0x4000;
    loader.load_memory_page(MemoryRange::Ram(0..pc_offset), rd.by_ref())?;
    let mut pc = [0u8;2];
    rd.read_exact(&mut pc)?;
    cpu.set_pc(u16::from_le_bytes(pc));
    let rest_offset = pc_offset + 2;
    if rest_offset < 0xC000 {
        loader.load_memory_page(MemoryRange::Ram(rest_offset..0xC000), rd)?;
    }
    loader.assign_cpu(CpuModel::NMOS(cpu));
    Ok(())
}

//! **SNA** file format utilities.
use std::convert::{TryInto};
use std::io::{ErrorKind, Error, Read, Result};

use spectrusty_core::{
   memory::ZxMemory,
   z80emu::{Cpu, Prefix, StkReg16, CpuFlags}
};
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
*/
#[derive(Clone, Copy, Debug, Default)]
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
   bytes: [u8;core::mem::size_of::<SnaHeader>()],
   header: SnaHeader
}
pub const SNA_LENGTH: usize = 49179;
/// Reads a *SNA* file and inserts its content into provided memory and configures the `Cpu`.
/// Returns border color on success.
pub fn read_sna<R: Read, M: ZxMemory, C: Cpu>(mut rd: R, cpu: &mut C, mem: &mut M) -> Result<u8> {
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
   let sp = u16::from_le_bytes(sna.sp);
   cpu.set_sp(sp.wrapping_add(2));
   cpu.set_im(sna.im.try_into().map_err(|_| {
      Error::new(ErrorKind::InvalidData, "Not a proper SNA block: invalid interrupt mode")
   })?);
   cpu.inc_r();
   cpu.inc_r(); // RETN would increase this 2 times
   mem.load_into_mem(0x4000..=0xFFFF, rd).map_err(|_| {
      Error::new(ErrorKind::InvalidData, "SNA: Need at least 48k RAM memory")
   })?;
   cpu.set_pc(mem.read16(sp));
   Ok(sna.border & 7)
}

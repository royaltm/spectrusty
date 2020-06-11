use std::io::{self, Read, Result};

use spectrusty_core::z80emu::{CpuFlags,Z80NMOS, StkReg16, Prefix, Cpu};
use spectrusty_core::chip::ReadEarMode;

use crate::snapshot::*;
use crate::StructRead;

use super::common::*;
use super::decompress::*;

// If bit 7 of byte 37 is set, the hardware types are modified slightly: any 48K machine becomes a 16K machine,
// any 128K machines becomes a +2 and any +3 machine becomes a +2A.
fn select_hw_model(version: Z80Version, head_ex: &HeaderEx) -> Option<(ComputerModel, Extensions)> {
    let hw_mode = head_ex.hw_mode;
    let flags3 = Flags3::from(head_ex.flags3);
    let mgt_type = head_ex.mgt_type;
    use ComputerModel::*;
    use Z80Version::*;
    Some(match (hw_mode, version) {
        (0, _) if flags3.is_alt_hw_mode() => (Spectrum16, Extensions::empty()),
        (0, _) => (Spectrum48, Extensions::empty()),
        (1, _) if flags3.is_alt_hw_mode() => (Spectrum16, Extensions::IF1),
        (1, _) =>  (Spectrum48, Extensions::IF1),
        (2, _) =>  (Spectrum48, Extensions::SAM_RAM),
        (3, V2)|(4, V3) if flags3.is_alt_hw_mode() => (SpectrumPlus2, Extensions::empty()),
        (3, V2)|(4, V3) => (Spectrum128, Extensions::empty()),
        (3, V3) if flags3.is_alt_hw_mode() && mgt_type == 16 => (Spectrum16, Extensions::PLUS_D),
        (3, V3) if flags3.is_alt_hw_mode() && mgt_type <= 1 => (Spectrum16, Extensions::DISCIPLE),
        (3, V3) if mgt_type == 16 => (Spectrum48, Extensions::PLUS_D),
        (3, V3) if mgt_type <= 1 => (Spectrum48, Extensions::DISCIPLE),
        (4, V2)|(5, V3) => (Spectrum128, Extensions::IF1),
        (6, V3) if mgt_type == 16 => (Spectrum128, Extensions::PLUS_D),
        (6, V3) if mgt_type <= 1 => (Spectrum128, Extensions::DISCIPLE),
        (7, _)|(8, _) if flags3.is_alt_hw_mode() => (SpectrumPlus2A, Extensions::empty()),
        (7, _)|(8, _) => (SpectrumPlus3, Extensions::empty()),
        // (9, _)   => (Pentagon128, Extensions::empty()),
        // (10, _)  => (Scorpion256, Extensions::empty()),
        // (11, _)  => (DidaktikKompakt, Extensions::empty()),
        (12, _)  => (SpectrumPlus2, Extensions::empty()),
        (13, _)  => (SpectrumPlus2A, Extensions::empty()),
        (14, _)  => (TimexTC2048, Extensions::empty()),
        (15, _)  => (TimexTC2068, Extensions::empty()),
        (128, _) => (TimexTS2068, Extensions::empty()),
        _ => return None
    })
}

fn select_ay_model(model: ComputerModel, flags3: Flags3) -> Option<Ay3_891xDevice> {
    use ComputerModel::*;
    match model {
        Spectrum128|SpectrumPlus2|
        SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
        SpectrumSE => {
            Some(Ay3_891xDevice::Ay128k)
        }
        TimexTC2068|TimexTS2068 => Some(Ay3_891xDevice::Timex),
        _ if flags3.is_ay_fuller_box() => Some(Ay3_891xDevice::FullerBox),
        _ if flags3.is_ay_melodik() => Some(Ay3_891xDevice::Melodik),
        _ => None
    }
}

fn mem_page_to_range(page: u8, model: ComputerModel, ext: Extensions) -> Option<MemoryRange> {
    use ComputerModel::*;
    match model {
        Spectrum16|Spectrum48|
        TimexTC2048|TimexTS2068|TimexTC2068 => {
            Some(match page {
                 0 => MemoryRange::Rom(0..PAGE_SIZE),
                 1 if ext.intersects(Extensions::IF1) => MemoryRange::Interface1Rom,
                 1 if ext.intersects(Extensions::PLUS_D) => MemoryRange::PlusDRom,
                 1 if ext.intersects(Extensions::DISCIPLE) => MemoryRange::DiscipleRom,
                 2 if ext.intersects(Extensions::SAM_RAM) => MemoryRange::SamRamRom(0..PAGE_SIZE),
                 3 if ext.intersects(Extensions::SAM_RAM) => MemoryRange::SamRamRom(PAGE_SIZE..2*PAGE_SIZE),
                 4 => MemoryRange::Ram(  PAGE_SIZE..2*PAGE_SIZE),
                 5 => MemoryRange::Ram(2*PAGE_SIZE..3*PAGE_SIZE),
                 6 if ext.intersects(Extensions::SAM_RAM) => MemoryRange::Ram(3*PAGE_SIZE..4*PAGE_SIZE),
                 7 if ext.intersects(Extensions::SAM_RAM) => MemoryRange::Ram(4*PAGE_SIZE..5*PAGE_SIZE),
                 8 => MemoryRange::Ram(0..PAGE_SIZE),
                11 => MemoryRange::MultifaceRom,
                 _ => return None
            })
        }
        Spectrum128|SpectrumPlus2|
        SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
        SpectrumSE => {
            Some(match page {
                0 => MemoryRange::Rom(PAGE_SIZE..2*PAGE_SIZE),
                1 if ext.intersects(Extensions::IF1) => MemoryRange::Interface1Rom,
                1 if ext.intersects(Extensions::PLUS_D) => MemoryRange::PlusDRom,
                1 if ext.intersects(Extensions::DISCIPLE) => MemoryRange::DiscipleRom,
                2 => MemoryRange::Rom(0..PAGE_SIZE),
                3..=10 => {
                    let address = (page - 3) as usize * PAGE_SIZE;
                    MemoryRange::Ram(address..address + PAGE_SIZE)
                }
                11..=18 if model == SpectrumSE => {
                    let address = (page - 11) as usize * PAGE_SIZE;
                    MemoryRange::Ram(address..address + PAGE_SIZE)                    
                }
                11 => MemoryRange::MultifaceRom,
                 _ => return None
            })
        }
        _ => None
    }
}

fn create_cpu(head: &Header) -> Result<Z80NMOS> {
    let mut cpu = Z80NMOS::default();
    cpu.reset();
    cpu.set_i(head.i);
    cpu.set_reg16(StkReg16::HL, u16::from_le_bytes(head.hl_alt));
    cpu.set_reg16(StkReg16::DE, u16::from_le_bytes(head.de_alt));
    cpu.set_reg16(StkReg16::BC, u16::from_le_bytes(head.bc_alt));
    cpu.exx();
    cpu.set_acc(head.a_alt);
    cpu.set_flags(CpuFlags::from_bits_truncate(head.f_alt));
    cpu.ex_af_af();
    cpu.set_reg16(StkReg16::HL, u16::from_le_bytes(head.hl));
    cpu.set_reg16(StkReg16::DE, u16::from_le_bytes(head.de));
    cpu.set_reg16(StkReg16::BC, u16::from_le_bytes(head.bc));
    cpu.set_index16(Prefix::Yfd, u16::from_le_bytes(head.iy));
    cpu.set_index16(Prefix::Xdd, u16::from_le_bytes(head.ix));
    cpu.set_iffs(head.iff1 != 0, head.iff2 != 0);
    cpu.set_r(Flags1::from(head.flags1).mix_r(head.r7));
    cpu.set_acc(head.a);
    cpu.set_flags(CpuFlags::from_bits_truncate(head.f));
    cpu.set_sp(u16::from_le_bytes(head.sp));
    cpu.set_im(Flags2::from(head.flags2).interrupt_mode()?);
    cpu.set_pc(u16::from_le_bytes(head.pc));
    Ok(cpu)
}

/// Loads a **Z80** file into the provided snapshot `loader` from a source.
pub fn load_z80<R: Read, S: SnapshotLoader>(
        mut rd: R,
        loader: &mut S
    ) -> Result<()>
{
    use ComputerModel::*;

    let header = Header::read_new_struct(rd.by_ref())?;
    let mut version = Z80Version::V1;

    let mut model = ComputerModel::Spectrum48;
    let mut extensions = Extensions::default();
    let mut cpu = create_cpu(&header)?;

    let header_ex = if cpu.get_pc() == 0 {
        let (ver, head_ex) = load_header_ex(rd.by_ref())?;
        version = ver;
        cpu.set_pc(u16::from_le_bytes(head_ex.pc));
        let (mdl, ext) = select_hw_model(version, &head_ex).ok_or_else(||
            io::Error::new(io::ErrorKind::InvalidData, "unsupported model")
        )?;
        model = mdl;
        extensions = ext;
        Some(head_ex)
    }
    else {
        None
    };

    let flags1 = Flags1::from(header.flags1);
    let border = flags1.border_color();
    let flags2 = Flags2::from(header.flags2);
    let joystick = flags2.joystick_model();
    let issue = model.applicable_issue(
        if flags2.is_issue2_emulation() {
            ReadEarMode::Issue2
        }
        else {
            ReadEarMode::Issue3
        }
    );

    loader.select_model(model, extensions, border, issue)
          .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    loader.select_joystick(joystick);
    loader.assign_cpu(CpuModel::NMOS(cpu));

    let mut buf = Vec::new();
    if version == Z80Version::V1 {
        if flags1.is_mem_compressed() {
            rd.read_to_end(&mut buf)?;
            let buf = match buf.get(buf.len() - 4..) {
                Some(MEMORY_V1_TERM) => &buf[..buf.len() - 4],
                _ => &buf[..]
            };
            let decompress = MemDecompress::new(buf);
            loader.read_into_memory(MemoryRange::Ram(0..3*PAGE_SIZE), decompress)?;
        }
        else {
            loader.read_into_memory(MemoryRange::Ram(0..3*PAGE_SIZE), rd)?;
        }
    }
    else {
        while let Some((len, page, is_compressed)) = load_mem_header(rd.by_ref())? {
            let range = mem_page_to_range(page, model, extensions).ok_or_else(||
                io::Error::new(io::ErrorKind::InvalidData, "unsupported memory page")
            )?;
            if is_compressed {
                buf.resize(len, 0);
                rd.read_exact(&mut buf)?;
                let decompress = MemDecompress::new(&buf);
                loader.read_into_memory(range, decompress)?;
            }
            else {
                loader.read_into_memory(range, rd.by_ref().take(len as u64))?;
            }
        }
    }

    if let Some(head_ex) = header_ex {
        if let Some(choice) = select_ay_model(model, Flags3::from(head_ex.flags3)) {
            loader.setup_ay(choice, head_ex.ay_sel_reg.into(), &head_ex.ay_regs);
        }

        if version == Z80Version::V3 {
            let ts = z80_to_cycles(u16::from_le_bytes(head_ex.ts_lo), head_ex.ts_hi, model);
            loader.set_clock(ts);
            let data = head_ex.port2;
            if data != 0 {
                match model {
                    SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
                    SpectrumSE => {
                        loader.write_port(0x1ffd, data);
                    }
                    _ => {}
                }
            }
        }

        match model {
            TimexTC2048|TimexTS2068|TimexTC2068 => {
                loader.write_port(0xf4, head_ex.port1);
            }
            Spectrum128|SpectrumPlus2|
            SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
            SpectrumSE => {
                loader.write_port(0x7ffd, head_ex.port1);
            }
            _ => {}
        }
        match model {
            TimexTC2048|TimexTS2068|TimexTC2068 => {
                loader.write_port(0xff, head_ex.ifrom);
            }
            Spectrum16|Spectrum48|
            Spectrum128|SpectrumPlus2|
            SpectrumPlus2A|SpectrumPlus3|SpectrumPlus3e|
            SpectrumSE
            if head_ex.ifrom == 0xff && extensions.intersects(Extensions::IF1) => {
                loader.interface1_rom_paged_in();
            }
            _ => {}
        }
    }
    Ok(())
}

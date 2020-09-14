//! Regression tests of the boot process of the selected ZX Spectrum machines.
use std::io::Read;
use spectrusty::chip::{*, ula::*, ula128::*, ula3::*};
use spectrusty::memory::*;
use spectrusty::clock::*;
use rand::prelude::*;
use spectrusty::z80emu::*;

const ROM48: &[u8] = include_bytes!("../resources/roms/48.rom");
const ROM128_0: &[u8] = include_bytes!("../resources/roms/128-0.rom");
const ROM128_1: &[u8] = include_bytes!("../resources/roms/128-1.rom");
const ROM_PLUS3_0: &[u8] = include_bytes!("../resources/roms/plus3-0.rom");
const ROM_PLUS3_1: &[u8] = include_bytes!("../resources/roms/plus3-1.rom");
const ROM_PLUS3_2: &[u8] = include_bytes!("../resources/roms/plus3-2.rom");
const ROM_PLUS3_3: &[u8] = include_bytes!("../resources/roms/plus3-3.rom");

const CPU_RESET: &str = r#"{
  "af": -1,
  "afAlt": -1,
  "regs": {
    "bc": 0,
    "de": 0,
    "hl": 0
  },
  "regsAlt": {
    "bc": 0,
    "de": 0,
    "hl": 0
  },
  "index": {
    "ix": 0,
    "iy": 0
  },
  "pc": 0,
  "sp": -1,
  "memptr": 0,
  "lastEi": false,
  "ir": 0,
  "im": "Mode0",
  "iff1": false,
  "iff2": false,
  "halt": false,
  "prefix": null,
  "r": 0,
  "flavour": {
    "flagsModified": false,
    "lastFlagsModified": false
  }
}"#;

const CPU_BOOT_16K: &str = r#"{
  "af": [24,0],
  "afAlt": [68,0],
  "regs": {
    "bc": 0,
    "de": [185,92],
    "hl": [168,16]
  },
  "regsAlt": {
    "bc": [75,23],
    "de": [6,0],
    "hl": [127,16]
  },
  "index": {
    "ix": 0,
    "iy": [58,92]
  },
  "pc": [168,16],
  "sp": [72,127],
  "memptr": [44,22],
  "lastEi": false,
  "ir": [0,63],
  "im": "Mode1",
  "iff1": true,
  "iff2": true,
  "halt": false,
  "prefix": null,
  "r": 134,
  "flavour": {
    "flagsModified": false,
    "lastFlagsModified": false
  }
}"#;

const CPU_BOOT_48K: &str = r#"{
  "af": [24,0],
  "afAlt": [68,0],
  "regs": {
    "bc": 0,
    "de": [185,92],
    "hl": [168,16]
  },
  "regsAlt": {
    "bc": [75,23],
    "de": [6,0],
    "hl": [127,16]
  },
  "index": {
    "ix": 0,
    "iy": [58,92]
  },
  "pc": [168,16],
  "sp": [72,255],
  "memptr": [44,22],
  "lastEi": false,
  "ir": [0,63],
  "im": "Mode1",
  "iff1": true,
  "iff2": true,
  "halt": false,
  "prefix": null,
  "r": 130,
  "flavour": {
    "flagsModified": false,
    "lastFlagsModified": false
  }
}"#;

const CPU_BOOT_128K: &str = r#"{
  "af": "0044",
  "afAlt": "0044",
  "regs": {
    "bc": "0100",
    "de": "2f6f",
    "hl": "70a7"
  },
  "regsAlt": {
    "bc": "0a1a",
    "de": "0007",
    "hl": "ffff"
  },
  "index": {
    "ix": "fd6c",
    "iy": "5c3a"
  },
  "pc": "2653",
  "sp": "5bff",
  "memptr": "2653",
  "lastEi": false,
  "ir": 0,
  "im": "Mode1",
  "iff1": true,
  "iff2": true,
  "halt": false,
  "prefix": null,
  "r": 248,
  "flavour": {
    "flagsModified": false,
    "lastFlagsModified": false
  }
}"#;

const CPU_BOOT_PLUS3: &str = r#"{
  "af": "0044",
  "afAlt": "0044",
  "regs": {
    "bc": "0100",
    "de": "276f",
    "hl": "70a7"
  },
  "regsAlt": {
    "bc": "0b1a",
    "de": "0007",
    "hl": "ffff"
  },
  "index": {
    "ix": "fd98",
    "iy": "5c3a"
  },
  "pc": "0703",
  "sp": "5bff",
  "memptr": "0703",
  "lastEi": false,
  "ir": 0,
  "im": "Mode1",
  "iff1": true,
  "iff2": true,
  "halt": false,
  "prefix": null,
  "r": 60,
  "flavour": {
    "flagsModified": false,
    "lastFlagsModified": false
  }
}"#;

const KEYBOARD_INPUT: u16      = 0x10A8;
const MAIN_WAIT_KEY_128: u16   = 0x2653;
const MAIN_WAIT_KEY_PLUS3: u16 = 0x0703;

#[allow(dead_code)]
const NO_DEBUG: Option<CpuDebugFn> = None;

fn debug_cpu(deb: CpuDebug) {
    println!("{:X}", deb);
}

fn boot_spectrum<U, R: Read>(rom: R, stop_pc: u16, nframes: u64, init_fts: FTs, expect_cpu: &str, expect_fts: FTs)
    where U: UlaCommon + Default
{
    let mut cpu = Z80NMOS::default();
    let mut ula = U::default();
    cpu.reset();
    ula.memory_mut().fill_mem(.., random).unwrap();
    ula.memory_mut().load_into_rom(rom).unwrap();
    ula.set_frame_tstate(init_fts);
    assert_eq!(cpu, serde_json::from_str(&CPU_RESET).unwrap());
    for _ in 0..nframes {
        let _ = ula.execute_next_frame(&mut cpu);
    }
    while cpu.get_pc() != stop_pc {
        let _ = ula.execute_single_step(&mut cpu, Some(debug_cpu));
    }
    assert_eq!(ula.current_frame(), nframes);
    assert_eq!(ula.current_tstate(), expect_fts);
    assert_eq!(cpu, serde_json::from_str(&expect_cpu).unwrap());
}

#[test]
fn test_boot() {
    boot_spectrum::<UlaPAL<Memory16k>, _>(ROM48, KEYBOARD_INPUT, 47, 16, CPU_BOOT_16K, 16104);
    boot_spectrum::<UlaPAL<Memory48k>, _>(ROM48, KEYBOARD_INPUT, 86, 9, CPU_BOOT_48K, 40249);
    let rom128 = ROM128_0.chain(ROM128_1);
    boot_spectrum::<Ula128, _>(rom128, MAIN_WAIT_KEY_128, 66, 0, CPU_BOOT_128K, 37904);
    let rom_plus3 = ROM_PLUS3_0.chain(ROM_PLUS3_1).chain(ROM_PLUS3_2).chain(ROM_PLUS3_3);
    boot_spectrum::<Ula3, _>(rom_plus3, MAIN_WAIT_KEY_PLUS3, 88, 0, CPU_BOOT_PLUS3, 15563);
}


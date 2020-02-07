use z80emu::*;

// Let's use the simple T-state counter.
type TsClock = host::TsCounter<u32>;

// Let's add some memory.
#[derive(Clone, Debug, Default)]
struct Mem<'a>(u16, &'a [u8]);

impl Io for Mem<'_> {
    type Timestamp = u32;
    type WrIoBreak = ();
    type RetiBreak = ();
}

impl Memory for Mem<'_> {
    type Timestamp = u32;
    fn read_debug(&self, addr: u16) -> u8 {
        *self.1.get(addr.wrapping_sub(self.0) as usize).unwrap_or(&0xff)
    }
}

pub fn debug_memory<F: Fn((CpuDebug, u32))>(pc: u16, mem: &[u8], debug: F) {
    let mut cpu = Z80NMOS::default();
    let mut cursor: usize = 0;
    let mut dbg: Option<CpuDebug> = None;
    while cursor < mem.len() {
        let mut tsc = TsClock::default();
        let pc = pc.wrapping_add(cursor as u16);
        cpu.reset();
        cpu.set_pc(pc);
        let _ = cpu.execute_next(&mut Mem(pc, &mem[cursor..]), &mut tsc, Some(|deb: CpuDebug| {
            cursor += deb.pc.wrapping_sub(pc) as usize + deb.code.len();
            dbg = Some(deb);
        }));
        if let Some(deb) = dbg.take() {
            debug((deb, tsc.as_timestamp()));
        }
        if cpu.is_after_prefix() {
            break;
        }
    }
}

pub fn print_debug_memory(pc: u16, mem: &[u8]) {
    debug_memory(pc, mem, |(deb, ts)| {
        println!("{:x} (t:{})", deb, ts);
    });
}

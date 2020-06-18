//! Performs tests of the CPU instructions and contention emulation using tests from the [Fuse]
//! ZX Spectrum emulator.
//!
//! To run these tests, "tests.in" and "tests.expected" source files are needed to be present in the
//! "tests/fuse" directory.
//! This program attempts to download them before running all the tests if the files are not present.
//!
//! [Fuse]: https://sourceforge.net/projects/fuse-emulator/
use core::num::{NonZeroU8, NonZeroU16};
use std::cell::RefCell;
use std::error::Error;
use std::convert::TryInto;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::io::{Read, BufReader, BufRead, ErrorKind};
use std::fs::{File, OpenOptions};

use spectrusty::z80emu::*;
use spectrusty::clock::{FTs, Ts, VideoTs, VFrameTsCounter};
use spectrusty_core::ula_io_contention;
use spectrusty::chip::ula::{UlaVideoFrame, ULA_CONTENTION_MASK};

type DynResult<T> = Result<T, Box<dyn Error>>;

// here, the current test case's expected events are being consumed from
thread_local!(static EVENTS: ExpectedEvents = RefCell::new(VecDeque::new()));
type ExpectedEvents = RefCell<VecDeque<Event>>;
// internal clock type
type VideoCounter = VFrameTsCounter<UlaVideoFrame>;

// Fuse tests are crafted for NMOS flavour
type CPU = Z80NMOS;

// implements Io and Memory
#[derive(Clone, Debug)]
struct TestCase {
    name: String,
    // CMOS has no internal flavour state, it's better for CPU comparison
    // since "tests.expected" contains no corresponding flavour state
    cpu: Z80CMOS,
    tsc: TestCounter,
    mem: TestMemory,
}

// implements Clock
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TestCounter {
    vtsc: VideoCounter
}

impl Default for TestCounter {
    fn default() -> Self {
        let vtsc = VideoCounter::from_video_ts(VideoTs::default(), ULA_CONTENTION_MASK);
        TestCounter { vtsc }
    }
}

// dynamically adjusted, fragemented memory
#[derive(Clone, Debug, PartialEq)]
struct TestMemory {
    frags: Vec<(usize, Vec<u8>)>
}

// expected event, e.g.: "10 MR 0002 9c"
#[derive(Clone, Copy, Debug, PartialEq)]
struct Event(VideoTs, EventType);

// expected event type, e.g.: "MR 0002 9c"
#[derive(Clone, Copy, Debug, PartialEq)]
enum EventType {
    // MR <address> <data>
    MemRead(u16, u8),
    // MW <address> <data>
    MemWrite(u16, u8),
    // MC <address>
    MemContend(u16),
    // PR <address> <data>
    PortRead(u16, u8),
    // PW <address> <data>
    PortWrite(u16, u8),
    // PC <address>
    PortContend(u16),
}

impl TestCase {
    fn run(mut self, expected: TestCase, events: VecDeque<Event>) {
        print!("{:14}: ", self.name);
        assert_eq!(self.name, expected.name);
        EVENTS.with(|ev| *ev.borrow_mut() = events);
        let mut tsc = TestCounter::default();
        let mut cpu: CPU = self.cpu.clone().into_flavour();
        loop {
            let _ = cpu.execute_next(&mut self, &mut tsc, Some(debug_cpu));
            if cpu.is_after_prefix() {
                continue;
            }
            if tsc < self.tsc {
                print!("{:16}", "");
            }
            else {
                break;
            }
        }
        Self::check_results(cpu, tsc, self.mem, expected);
    }

    fn run_with_limit(mut self, expected: TestCase, events: VecDeque<Event>) {
        assert_eq!(self.name, expected.name);
        EVENTS.with(|ev| *ev.borrow_mut() = events);
        let mut tsc = TestCounter::default();
        let mut cpu: CPU = self.cpu.clone().into_flavour();
        let mut ts_limit = self.tsc.into();
        loop {
            let _ = cpu.execute_with_limit(&mut self, &mut tsc, ts_limit);
            if cpu.is_after_prefix() {
                // execute_with_limit exits immediately when limit has been reached
                let mut vtsc = tsc.vtsc;
                vtsc += 1;
                ts_limit = *vtsc;
                continue;
            }
            if tsc >= self.tsc {
                break;
            }
        }
        Self::check_results(cpu, tsc, self.mem, expected);
    }

    fn check_results(mut cpu: CPU, tsc: TestCounter, mem: TestMemory, mut expected: TestCase) {
        cpu.normalize_r();
        if cpu.is_after_ei() {
            assert_eq!(expected.cpu.get_iffs(), (true, true));
            // there is no corresponding state of the cpu.last_ei flag in "tests.in"
            expected.cpu.enable_interrupts();
        }
        let cpu = cpu.into_flavour();
        assert_eq!(expected.cpu, cpu);
        assert_eq!(expected.tsc, tsc);
        expected.mem.matches(mem);
    }

    fn parse_in<R: BufRead>(mut input: R) -> DynResult<Option<TestCase>> {
        let mut lines = input.by_ref().lines();
        lines.next().map(|name| {
            let name = name?;
            if name.len() == 0 { Err("missing input test case name")? }
            let regs = lines.next().ok_or("missing input CPU registers")??;
            let meta = lines.next().ok_or("missing input CPU configuration")??;
            parse_cpu_tsc(regs, meta).map(|(cpu, tsc)| (name, cpu, tsc))
        })
        .transpose()?
        .map(|(name, cpu, tsc)| {
            let mem = read_parse_mem(input)?;
            let cpu = cpu.into_flavour();
            Ok(TestCase { name, cpu, tsc, mem })
        })
        .transpose()
    }

    fn parse_expected<R: BufRead>(mut input: R) -> DynResult<Option<(TestCase, VecDeque<Event>)>> {
        let mut lines = input.by_ref().lines();
        lines.next().map(|name| {
            let name = name?;
            if name.len() == 0 { Err("missing expected test case name")? }
            let mut events = VecDeque::new();
            let mut regs: Option<String> = None;
            for line in lines.by_ref() {
                let line = line?;
                if line.len() < 64 {
                    events.push_back(parse_event(line)?);
                }
                else {
                    regs = Some(line);
                    break;
                }
            }
            let regs = regs.ok_or("missing expected CPU registers")?;
            let meta = lines.next().ok_or("missing expected CPU configuration")??;
            parse_cpu_tsc(regs, meta).map(|(cpu, tsc)| (name, cpu, tsc, events))
        })
        .transpose()?
        .map(|(name, cpu, tsc, events)| {
            let mem = read_parse_mem(input)?;
            let cpu = cpu.into_flavour();
            Ok((TestCase { name, cpu, tsc, mem }, events))
        })
        .transpose()
    }
}

fn parse_cpu_tsc(regs: String, meta: String) -> DynResult<(CPU, TestCounter)> {
    let mut iter = regs.split_ascii_whitespace();
    let regs = iter.by_ref().take(13)
                            .map(|s| u16::from_str_radix(s, 16))
                            .collect::<Result<Vec<_>,_>>()?;
    if regs.len() != 13 || iter.next().is_some() {
        Err(format!("Incorrect number of the CPU registers: {}", regs.len()))?;
    }
    let mut iter = meta.split_ascii_whitespace();
    let ir = iter.by_ref().take(2)
                          .map(|s| u8::from_str_radix(s, 16))
                          .collect::<Result<Vec<_>,_>>()?;
    let iffs = iter.by_ref().take(2)
                            .map(|s| u8::from_str_radix(s, 2))
                            .collect::<Result<Vec<_>,_>>()?;
    if ir.len() != 2 || iffs.len() != 2 { Err("Missing CPU data")?; }
    let im = u8::from_str_radix(iter.next().ok_or("Missing IM")?, 3)?;
    let halted = u8::from_str_radix(iter.next().ok_or("Missing <halted>")?, 2)?;
    let ts = FTs::from_str_radix(iter.next().ok_or("Missing <tstates>")?, 10)?;
    if iter.next().is_some() {
        Err("Extraneous data after the CPU configuration")?
    }
    let tsc = TestCounter::from(ts);
    let mut cpu = CPU::default();
    cpu.set_reg16(StkReg16::AF, regs[4]);
    cpu.set_reg16(StkReg16::BC, regs[5]);
    cpu.set_reg16(StkReg16::DE, regs[6]);
    cpu.set_reg16(StkReg16::HL, regs[7]);
    cpu.ex_af_af();
    cpu.exx();
    cpu.set_reg16(StkReg16::AF, regs[0]);
    cpu.set_reg16(StkReg16::BC, regs[1]);
    cpu.set_reg16(StkReg16::DE, regs[2]);
    cpu.set_reg16(StkReg16::HL, regs[3]);
    cpu.set_index16(Prefix::Xdd, regs[8]);
    cpu.set_index16(Prefix::Yfd, regs[9]);
    cpu.set_sp(regs[10]);
    cpu.set_pc(regs[11]);
    cpu.set_memptr(regs[12]);
    cpu.set_i(ir[0]);
    cpu.set_r(ir[1]);
    cpu.set_iffs(iffs[0]!=0, iffs[1]!=0);
    cpu.set_im(im.try_into().map_err(|_| "Unknown interrupt mode")?);
    if halted != 0 {
        cpu.halt();
    }
    Ok((cpu, tsc))
}

fn read_parse_mem<R: BufRead>(input: R) -> DynResult<TestMemory> {
    let mut mem = TestMemory { frags: Vec::new() };
    for line in input.lines() {
        if let Some((start, data)) = parse_mem_fragment(line?)? {
            mem.frags.push((start as usize, data));
        }
        else {
            break;
        }
    }
    Ok(mem)
}

fn parse_mem_fragment(line: String) -> DynResult<Option<(u16, Vec<u8>)>> {
    let mut iter = line.split_ascii_whitespace()
                       .take_while(|s| *s != "-1");
    iter.next().map(|s| {
        let addr = u16::from_str_radix(s, 16)?;
        let data = iter.map(|s| u8::from_str_radix(s, 16))
                       .collect::<Result<Vec<_>,_>>()?;
        Ok((addr, data))
    })
    .transpose()
}

fn parse_event(line: String) -> DynResult<Event> {
    let mut iter = line.split_ascii_whitespace();
    let ts = FTs::from_str_radix(iter.next().ok_or("Missing event <time>")?, 10)?;
    let tsc = TestCounter::from(ts);
    let evt = iter.next().ok_or("Missing event <type>")?;
    let addr = u16::from_str_radix(iter.next().ok_or("Missing event <address>")?, 16)?;
    let mut get_data = || -> DynResult<u8> {
        u8::from_str_radix(iter.next().ok_or("Missing event <data>")?, 16)
        .map_err(From::from)
    };
    use EventType::*;
    let event_type = match evt {
        "MR" => MemRead(addr, get_data()?),
        "MW" => MemWrite(addr, get_data()?),
        "MC" => MemContend(addr),
        "PR" => PortRead(addr, get_data()?),
        "PW" => PortWrite(addr, get_data()?),
        "PC" => PortContend(addr),
        _ => Err("Unknown event <type>")?
    };
    Ok(Event(tsc.into(), event_type))
}

// fetch the next expected event from thread global EVENTS
fn next_event() -> Option<Event> {
    EVENTS.with(|events| events.borrow_mut().pop_front())
}

// prepend the next expected event to the thread global EVENTS
fn prepend_event(ev: Event) {
    EVENTS.with(|events| events.borrow_mut().push_front(ev))
}

/**** Cpu trait implementations ****/

impl Io for TestCase {
    type Timestamp = VideoTs;
    type WrIoBreak = ();
    type RetiBreak = ();

    fn is_irq(&mut self, _tsc: VideoTs) -> bool {
        false
    }

    fn read_io(&mut self, port: u16, vts: Self::Timestamp) -> (u8, Option<NonZeroU16>) {
        let value = (port >> 8) as u8;
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(vts, EventType::PortRead(port, value)));
            }
            None => {
                assert!(false, "unexpected IO read");
            }
        }
        (value, None)
    }

    fn write_io(&mut self, port: u16, value: u8, vts: VideoTs)-> (Option<Self::WrIoBreak>, Option<NonZeroU16>) {
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(vts, EventType::PortWrite(port, value)));
            }
            None => {
                assert!(false, "unexpected IO write");
            }
        }
        (None, None)
    }

}

impl Memory for TestCase {
    type Timestamp = VideoTs;

    fn read_mem(&self, addr: u16, vts: VideoTs) -> u8 {
        let value = self.mem.read(addr);
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(vts, EventType::MemRead(addr, value)));
            }
            None => {
                assert!(false, "unexpected memory read");
            }
        }
        value
    }

    fn read_mem16(&self, addr: u16, vts: VideoTs) -> u16 {
        let value_lo = self.mem.read(addr);
        // 16 bit reads expected are: MC MR MC MR in z80emu it's: MC MR16 MC
        // expect MR MC MR
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(vts, EventType::MemRead(addr, value_lo)));
            }
            None => {
                assert!(false, "unexpected memory read (LSB)");
            }
        }
        let addr = addr.wrapping_add(1);
        let mc_evt = Event(vts, EventType::MemContend(addr));
        let mut tsc = TestCounter::from(vts);
        // expect MC MR
        tsc.add_mreq(addr);
        let value_hi = self.mem.read(addr);
        // expect MR
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(tsc.into(), EventType::MemRead(addr, value_hi)));
            }
            None => {
                assert!(false, "unexpected memory read (MSB)");
            }
        }
        // prepend MC
        prepend_event(mc_evt);
        u16::from_le_bytes([value_lo, value_hi])
    }

    fn read_opcode(&mut self, pc: u16, _ir: u16, vts: VideoTs) -> u8 {
        let value = self.mem.read(pc);
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(vts, EventType::MemRead(pc, value)));
            }
            None => {
                assert!(false, "unexpected opcode read");
            }
        }
        value
    }

    fn write_mem(&mut self, address: u16, value: u8, vts: VideoTs) {
        self.mem.write(address, value);
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(vts, EventType::MemWrite(address, value)));
            }
            None => {
                assert!(false, "unexpected memory write");
            }
        }
    }

    fn read_debug(&self, addr: u16) -> u8 {
        self.mem.read(addr)
    }
}

impl From<FTs> for TestCounter {
    fn from(ts: FTs) -> Self {
        let vtsc = VideoCounter::from_tstates(ts, ULA_CONTENTION_MASK);
        TestCounter { vtsc }
    }
}

impl From<VideoTs> for TestCounter {
    fn from(vts: VideoTs) -> Self {
        let vtsc = VideoCounter::from_video_ts(vts, ULA_CONTENTION_MASK);
        TestCounter { vtsc }
    }
}

impl From<TestCounter> for VideoTs {
    fn from(tsc: TestCounter) -> VideoTs {
        tsc.vtsc.into()
    }
}

impl Clock for TestCounter {
    type Limit = VideoTs;
    type Timestamp = VideoTs;

    fn is_past_limit(&self, limit: VideoTs) -> bool {
        self.as_timestamp() >= limit
    }

    fn as_timestamp(&self) -> VideoTs {
        *self.vtsc
    }

    fn add_wait_states(&mut self, _bus: u16, _wait_states: NonZeroU16) {
        unreachable!("unexpected wait states")
    }

    fn add_irq(&mut self, _address: u16) -> VideoTs {
        unreachable!("unexpected irq")
    }

    fn add_mreq(&mut self, address: u16) -> VideoTs {
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(*self.vtsc, EventType::MemContend(address)));
            }
            None => {
                assert!(false, "unexpected memory contention (MREQ)");
            }
        }
        self.vtsc.add_mreq(address)
    }

    fn add_no_mreq(&mut self, address: u16, add_ts: NonZeroU8) {
        // The z80emu differentiates the MREQ and internal cycles, but in Fuse the internal cycles are hardcoded
        // in its Z80 emulation as a series of MC.
        let mut vtsc = self.vtsc;
        for _ in 0..add_ts.get() {
            match next_event() {
                Some(ev) => {
                    assert_eq!(ev, Event(*vtsc, EventType::MemContend(address)));
                }
                None => {
                    assert!(false, "unexpected memory contention (NO MREQ)");
                }
            }
            vtsc += 1;
        }
        self.vtsc.add_no_mreq(address, add_ts);
        assert_eq!(vtsc, self.vtsc);
    }

    fn add_m1(&mut self, address: u16) -> VideoTs {
        match next_event() {
            Some(ev) => {
                assert_eq!(ev, Event(*self.vtsc, EventType::MemContend(address)));
            }
            None => {
                assert!(false, "unexpected memory contention (M1)");
            }
        }
        self.vtsc.add_m1(address)
    }

    fn add_io(&mut self, port: u16) -> VideoTs {
        // The port contention (PC) in Fuse is hard-coded in its Z80 emulation. In z80emu the contention
        // is delegated to Clock methods which depend on the VideoFrame and MemoryContention traits
        // and Clock::add_io is being always invoked only once before the IO::read_io (PR) or IO::write_io (PW).
        let VideoTs { vc, mut hc } = *self.vtsc;
        // Collect the contention event timestamps from the ula_io_contention macro which is used internally
        // by the Clock::add_io implementation for VFrameTsCounter.
        let mut contentions_vts = Vec::with_capacity(4);
        let mut contention = |hc| {
            contentions_vts.push(VideoCounter::new(vc, hc, ULA_CONTENTION_MASK).as_timestamp());
            hc
        };
        // ula_io_contention! modifies `hc`.
        let hc_end = ula_io_contention!(ULA_CONTENTION_MASK, port, hc, contention);
        // The timestamp when the PR/PW operation takes place.
        let vts_op = VideoCounter::new(vc, hc, ULA_CONTENTION_MASK).as_timestamp();
        assert_eq!(vts_op, self.vtsc.add_io(port));
        // The timestamp after the complete cycle.
        assert_eq!(VideoCounter::new(vc, hc_end, ULA_CONTENTION_MASK), self.vtsc);
        // An expected PR/PW event will be optionally captured below.
        let mut prw_evt: Option<Event> = None;
        for vts in contentions_vts.into_iter() {
            if vts == vts_op {
                assert!(prw_evt.is_none());
                prw_evt = next_event();
            }
            match next_event() {
                Some(ev) => {
                    assert_eq!(ev, Event(vts, EventType::PortContend(port)));
                }
                None => {
                    assert!(false, "unexpected port contention");
                }
            }
        }
        // Optionally prepend the captured in-between PW/PR event.
        if let Some(ev) = prw_evt {
            prepend_event(ev);
        }
        vts_op
    }
}

impl TestMemory {
    fn matches(self, other: Self) {
        for (mut addr, mut data) in self.frags {
            while !data.is_empty() {
                match other.chunk_ref(addr) {
                    Some(entry) => {
                        let len = data.len().min(entry.len());
                        assert_eq!(data[..len], entry[..len]);
                        data.drain(..len);
                        addr += len;
                    }
                    None => assert!(false, "missing {:x?}", (addr, data))
                }
            }
        }
    }

    fn chunk_ref(&self, addr: usize) -> Option<&[u8]> {
        self.frags.iter().find_map(|(start, data)|
            if addr >= *start && addr - *start < data.len() {
                Some(&data[addr - *start..])
            }
            else {
                None
            }
        )
    }

    fn read(&self, addr: u16) -> u8 {
        let addr = addr as usize;
        self.chunk_ref(addr).map(|data| data[0]).unwrap_or_else(||
            panic!("trying to read from undefined memory: <- ({:04X})", addr)
        )
    }

    fn write(&mut self, addr: u16, val: u8) {
        let addr = addr as usize;
        if let Some((start, data)) = self.frags.iter_mut()
                .find(|(start, data)| addr >= *start && addr - *start <= data.len()) {
            let index = addr - *start;
            match data.get_mut(index) {
                Some(p) => *p = val,
                None => data.push(val)
            }
        }
        else {
            self.frags.push((addr, vec![val]));
        }
    }
}

fn debug_cpu(deb: CpuDebug) {
    println!("{:x}", deb);
}

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), r"/tests/fuse");
const FUSE_URL: &str = "https://sourceforge.net/p/fuse-emulator/fuse/ci/master/tree/z80/tests/{{name}}?format=raw";

fn test_file_path(name: &str) -> PathBuf {
    let mut filepath = PathBuf::from(TESTS_DIR);
    filepath.push(name);
    filepath
}

fn open_file(name: &str) -> DynResult<BufReader<File>> {
    Ok(BufReader::new(File::open(test_file_path(name))?))
}

fn fetch_file(name: &str) -> DynResult<()> {
    let filepath = test_file_path(name);
    if let Some(mut file) = OpenOptions::new().write(true)
                                        .create_new(true)
                                        .open(filepath)
                                        .map(|f| Some(f))
                                        .or_else(|e| {
                                            if e.kind() == ErrorKind::AlreadyExists {
                                                Ok(None)
                                            }
                                            else {
                                                Err(e)
                                            }
                                        })? {
        let url = FUSE_URL.replace("{{name}}", name);
        println!("downloading: {}", url);
        let mut resp = reqwest::blocking::get(&url)?;
        resp.copy_to(&mut file)?;
    }
    Ok(())
}

fn ensure_test_files() -> DynResult<()> {
    for &name in &["README",
                   "tests.in",
                   "tests.expected"] {
        fetch_file(name)?
    }
    Ok(())
}

#[test]
#[ignore]
fn test_fuse() {
    ensure_test_files().unwrap();
    let mut tests_in = open_file("tests.in").expect("missing tests.in file");
    let mut tests_expected = open_file("tests.expected").expect("missing tests.expected file");
    let mut sep = String::new();
    while let Some(test_case) = TestCase::parse_in(tests_in.by_ref()).unwrap() {
        if let Some((expected_case, events)) = TestCase::parse_expected(tests_expected.by_ref()).unwrap() {
            test_case.clone().run(expected_case.clone(), events.clone());
            test_case.run_with_limit(expected_case, events);
        }
        else {
            assert!(false, "expected case is missing");
        }
        sep.clear();
        tests_in.read_line(&mut sep).expect("missing separator line");
        assert_eq!(sep.trim_end_matches('\n'), "");
    }
}

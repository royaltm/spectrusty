/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! *ZX Net* coders for the ZX Interface 1.
use core::mem;
use std::time::{Instant};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use spectrusty_core::clock::FrameTimestamp;
pub use super::zxnet_udp::*;

const CPU_HZ: f32 = 3_500_000.0;

/// This trait is being used by [ZxNet] to send and receive ZX-NET packets to and from remote parties.
pub trait ZxNetSocket {
    /// Should return a view of the current packet being composed or received.
    fn packet_data(&self) -> &[u8];
    /// Signals that the new packet will be composed for sending.
    fn begin_packet(&mut self);
    /// Adds a `byte` to the packet data. Should return the size of the packet data after appending `byte`.
    fn push_byte(&mut self, byte: u8) -> usize;
    /// Should return the index of the next byte to be pushed to the outbound data packet.
    /// This is the same as the byte size of the composed data packet so far.
    fn outbound_index(&self) -> usize;
    /// Should send the composed packet to the remote party.
    fn send_packet(&mut self);
    /// Should optionally wait and get the confirmation from the remote party.
    /// Returns `true` if the remote party confirmed the received packet.
    fn recv_accept(&mut self) -> bool;
    /// Should receive a data packet from the remote party.
    /// Returns `true` if the remote party has sent the next data packet.
    fn recv_packet(&mut self) -> bool;
    /// Gets the next byte from the last received data packet.
    /// Returns `None` if there are no more bytes to be returned.
    fn pull_byte(&mut self) -> Option<u8>;
    /// Should return the index of the next byte to be pulled from the outbound data packet.
    fn inbound_index(&self) -> usize;
    /// Should send the confirmation of the received packet to the remote party.
    fn send_accept(&mut self);
}

/// Implementation of this struct decodes and encodes ZX-NET packets from Spectrum's I/O port signals.
///
/// An implementation of [ZxNetSocket] should be provided as its `S` type parameter.
#[derive(Debug)]
pub struct ZxNet<T, S> {
    /// Direct access to the underlying [ZxNetSocket] implementation.
    pub socket: S,
    event_ts: T,
    dir_io: NetDir,
    io: NetState,
    net_state: bool
}

/// A helper struct for reading ZX-NET header information.
#[repr(C, packed)]
pub struct ZxNetHead {
    /// `NCIRIS` The destination station number.
    pub dest: u8,       
    /// `NCSELF` This Spectrum's station number.
    pub ours: u8,       
    /// `NCNUMB` The block number.
    pub serial: [u8;2], 
    /// `NCTYPE` The packet type code . 0 data, 1 EOF
    pub eof: u8,        
    /// `NCOBL` Number of bytes in data block.
    pub size: u8,       
    /// `NCDCS` The data checksum.
    pub dchk: u8,       
    /// `NCHCS` The header checksum.
    pub hchk: u8,       
}

pub(super) const HEAD_SIZE: usize = mem::size_of::<ZxNetHead>();

/// A trait for converting data to references of [ZxNetHead].
pub trait DataAsZxNetHead {
    /// Converts a reference to a data packet to a reference of [ZxNetHead].
    fn as_zxnet_header(&self) -> &ZxNetHead;
}

impl DataAsZxNetHead for [u8] {
    /// # Panics
    /// Panics if the slice length is less than the size of the [ZxNetHead] struct.
    fn as_zxnet_header(&self) -> &ZxNetHead {
        let head = &self[..mem::size_of::<ZxNetHead>()];
        let ptr = head.as_ptr() as *const ZxNetHead;
        unsafe { &*ptr }
    }
}

// const REST_DELAY: u32 = 6912 + 250; // input
// const SCOUT_WAIT_MAX: u32 = 47250;
const INPAK_WAIT_MAX: u32 = 8925;
const OUTPAK_START_DELAY: u32 = 110;
const BIT_DELAY: u32 = 60;
const REST_DELAY_THRESHOLD: u32 = 64;
const PROBE_DELAY_MIN: u32 = 65;
const PROBE_DELAY_MAX: u32 = 130;
const BYTE_DELAY: u32 = 120;
const BROADCAST_DATA_DELAY: i32 = 530;

// const MAX_PACKET_SIZE: usize = 256 + BODY_INDEX;

#[derive(Clone, Copy, Debug, PartialEq)]
enum NetDir {
    Inbound,
    Outbound
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NetState {
    Idle(u8),
    InputScout,
    InputStart,
    InputData(u8),
    OutputScout,
    OutputStart,
    OutputData(u8),
    OutputStop(u8),
    OutputEnd
}
// https://scratchpad.fandom.com/wiki/ZX_Net
// scout: 1 x x x x x x x 0 
//(2.5ms) 1 [ 0 x x x x x x x x 1 * bytes ] 0
impl<T: FrameTimestamp, S: ZxNetSocket> ZxNet<T, S> {
    pub fn send_state(&mut self, net: bool, timestamp: T) {
        match self.io {
            NetState::Idle(..) => {
                // println!("set scout: {} {} {}", net, V::vts_diff(self.event_ts, timestamp), V::vts_to_tstates(timestamp));
                self.net_state = net;
                self.io = NetState::OutputScout;
            }
            NetState::OutputScout if net && !self.net_state => { // OUTPAK will start
                // println!("OUTPAK start from scout");
                self.socket.begin_packet();
                self.dir_io = NetDir::Outbound;
                self.event_ts = timestamp + OUTPAK_START_DELAY;
                self.io = NetState::OutputStart;
            }
            NetState::OutputScout => {
                // println!("fake scout");
                self.net_state = false;
                self.io = NetState::Idle(0);
            }
            NetState::InputData(0) if net && timestamp >= self.event_ts => { // reply or body packet out OUTPAK
                // println!("reply or body packet OUTPAK: {}", V::vts_diff(self.event_ts, timestamp));
                self.event_ts = timestamp + OUTPAK_START_DELAY;
                self.io = NetState::OutputStart;
            }
            NetState::OutputStart if !net && timestamp < self.event_ts => {
                // println!("OUTPAK start: {}", V::vts_diff(self.event_ts, timestamp));
                self.event_ts = timestamp + BIT_DELAY;
                self.io = NetState::OutputData(0x80);
            }
            NetState::OutputData(bits) if timestamp < self.event_ts => {
                let next_bits = ((bits & !1)| u8::from(net)).rotate_right(1);
                self.event_ts = timestamp + BIT_DELAY;
                self.io = if bits & 1 == 1 {
                    NetState::OutputStop(next_bits)
                }
                else {
                    NetState::OutputData(next_bits)
                };
            }
            NetState::OutputStop(byte) if net && timestamp < self.event_ts => {
                // println!("OUTPAK stop: {:02x}", byte);
                match self.dir_io {
                    NetDir::Inbound if byte == 1 => {
                        // println!("got send resp 1");
                        if self.socket.inbound_index() == HEAD_SIZE {
                            // let now = Instant::now();
                            self.socket.send_accept();
                            // println!("sent accept in {:?}", now.elapsed());
                        }
                        self.io = NetState::OutputEnd;
                    }
                    NetDir::Inbound => {
                        // println!("this should be 1");
                        self.net_state = false;
                        self.io = NetState::Idle(0); // end of packet transmission                        
                    }
                    NetDir::Outbound => {
                        let len = self.socket.push_byte(byte);
                        self.io = if len == HEAD_SIZE {
                            // println!("outbound header end");
                            NetState::OutputEnd
                        }
                        else if len > HEAD_SIZE
                             && len - HEAD_SIZE == self.socket.packet_data().as_zxnet_header().size as usize {
                            // println!("outbound data end");
                            self.socket.send_packet();
                            NetState::OutputEnd
                        }
                        else {
                            NetState::OutputStart
                        }
                    }
                }
                self.event_ts = timestamp + BYTE_DELAY;
            }
            NetState::OutputEnd if !net && timestamp < self.event_ts => { // end outpack
                self.event_ts = timestamp; // TODO: SOME TIMEOUT
                let head = self.socket.packet_data().as_zxnet_header();
                match self.dir_io {
                    NetDir::Inbound => { // end of outpak resp
                        self.io = if self.socket.inbound_index() == head.size as usize + HEAD_SIZE {
                            self.net_state = false;
                            // println!("end of response and transmission");
                            NetState::Idle(0) // end of packet transmission                        
                        }
                        else {
                            // println!("end of response expecting more data");
                            self.net_state = true;
                            NetState::InputScout // expect DATA
                        }
                    }
                    NetDir::Outbound => {
                        self.io = if self.socket.outbound_index() == head.size as usize + HEAD_SIZE {
                            if head.dest == 0 { // BROADCAST - no response checking
                                // println!("end of BROADCAST");
                                self.net_state = false;
                                NetState::Idle(0) // end of packet transmission
                            }
                            else {
                                // println!("check remote response");
                                self.net_state = false;
                                NetState::InputScout // expect CHECK RESP need to fetch acceptance
                            }
                        }
                        else if head.dest == 0 { // BROADCAST - no response checking
                            // println!("BROADCAST: no check");
                            self.net_state = false;
                            NetState::InputData(0) // expect next OUTPAK
                        }
                        else {
                            // println!("check immediate response");
                            self.net_state = true;
                            NetState::InputScout // expect CHECK RESP
                        }
                    }
                }
            }
            _ => { // something wrong, let scout routine know it's not the right moment
                // println!("OUTPUT something wrong: {:?} {:?} {} {}", net, self.io, V::vts_to_tstates(timestamp), V::vts_diff(self.event_ts, timestamp));
                self.io = NetState::Idle(0);
                self.net_state = false;
            }
        }
    }

    pub fn poll_state(&mut self, timestamp: T) -> bool {
        match self.io {
            NetState::Idle(cnt) => {
                if timestamp >= self.event_ts {
                    match timestamp.diff_from(self.event_ts) as u32 {
                        0..=REST_DELAY_THRESHOLD => {
                            // println!("IDLE REST: {} {}", V::vts_diff(self.event_ts, timestamp), cnt);
                            self.event_ts = timestamp;
                            self.io = NetState::Idle(cnt.saturating_add(1));
                        }
                        PROBE_DELAY_MIN..=PROBE_DELAY_MAX if cnt < 191 => { // WAIT SCOUT
                            // let now = Instant::now();
                            if self.socket.recv_packet() {
                                // println!("{} got packet let it REST: {} {:?} {:?}", cnt,
                                //             self.socket.packet_data().len(), now.elapsed(), &self.socket.packet_data()[0..8]);
                                // got a packet, so regardless of what spectrums wants we will try to shove it
                                self.event_ts = timestamp;
                                self.io = NetState::InputScout;
                                self.dir_io = NetDir::Inbound;
                                self.net_state = true;
                            }
                            else {
                                // println!("READ PROBE: {}", V::vts_diff(self.event_ts, timestamp));
                                self.event_ts = timestamp;
                            }
                        }
                        _ => {
                            self.event_ts = timestamp;
                            self.io = NetState::Idle(0);
                            self.net_state = false;
                        }
                    }
                }
                else if cnt != 0 {
                    self.io = NetState::Idle(0);
                }
            }
            NetState::OutputScout => {
                // println!("verify scout");
                self.io = NetState::Idle(0);
            }
            NetState::InputScout if self.net_state => { // Spectrum ignores scout (maybe set a scout grace time...)
                // println!("SCOUT -> INPAK: {}", V::vts_diff(self.event_ts, timestamp));
                self.event_ts = timestamp;
                self.io = NetState::InputStart;
            }
            NetState::InputScout => {
                let now = Instant::now();
                if self.socket.recv_accept() {
                    // println!("got accept: {} {:?}", V::vts_diff(self.event_ts, timestamp), now.elapsed());
                    self.event_ts = timestamp;
                    self.io = NetState::InputStart;
                    self.net_state = true;
                }
                else {
                    // println!("NO ACCEPT!");
                    self.setup_event_time(timestamp, now);
                    self.io = NetState::Idle(0);
                }
            }
            NetState::InputStart => { // if this happens, something is wrong, so discard packet and go to idle
                // println!("INPAK no wait detected!");
                self.event_ts = timestamp + INPAK_WAIT_MAX;
                self.io = NetState::Idle(0);
                self.net_state = false;
            }
            NetState::InputData(byte) => {
                self.net_state = if timestamp < self.event_ts {
                    self.event_ts = timestamp + BIT_DELAY;
                    self.io = NetState::InputData(byte >> 1);
                    byte & 1 == 1
                }
                else if timestamp.diff_from(self.event_ts) <= BROADCAST_DATA_DELAY {
                    // println!("INPUT DATA BROADCAST?: {}", V::vts_diff(self.event_ts, timestamp));
                    self.event_ts = timestamp;
                    self.io = NetState::InputStart;
                    true
                }
                else {
                    // println!("INPUT DATA > IDLE: {}", V::vts_diff(self.event_ts, timestamp));
                    self.io = NetState::Idle(0);
                    false
                };
            }
            _ => { // unexpected during output
                // println!("INPUT something wrong: {:?} {} {}", self.io, V::vts_to_tstates(timestamp), V::vts_diff(self.event_ts, timestamp));
                self.io = NetState::Idle(0);
                self.net_state = false;
            }
        }
        self.net_state // set by whatever Spectrum writes
    }

    pub fn wait_data(&mut self, timestamp: T) {
        // println!("wait: {}", V::vts_to_tstates(timestamp));
        if let Some(byte) = match (self.io, self.dir_io) { // Spectrum wants a byte
                (NetState::InputStart, NetDir::Outbound) => Some(1),
                (NetState::InputStart, NetDir::Inbound) => self.socket.pull_byte(),
                (NetState::InputData(0), NetDir::Inbound) if timestamp > self.event_ts &&
                                        timestamp < self.event_ts + BIT_DELAY => {
                    // the whole byte has been transerred
                    self.socket.pull_byte()
                }
                _ => None
            }
        {
            // println!("WAIT -> BYTE [{}] {}", byte, V::vts_diff(self.event_ts, timestamp));
            self.event_ts = timestamp + 2*BIT_DELAY;
            self.io = NetState::InputData(byte);
        }
        else {
            // println!("bogus WAIT {}", V::vts_diff(self.event_ts, timestamp));
            self.event_ts = timestamp + INPAK_WAIT_MAX;
            self.io = NetState::Idle(0);
            self.net_state = false;
        }
    }

    pub fn next_frame(&mut self, _timestamp: T) {
        self.event_ts = self.event_ts.saturating_sub_frame();
    }

    fn setup_event_time(&mut self, timestamp: T, start: Instant) {
        let elapsed = start.elapsed().as_secs_f32();
        let elapsed_ts = (elapsed * CPU_HZ).round() as u32;
        // println!("waited: {} {}", elapsed_ts, elapsed);
        self.event_ts = timestamp + elapsed_ts;
    }
}

impl<T: Default, S: Default> Default for ZxNet<T, S> {
    fn default() -> Self {
        let socket = S::default();
        let event_ts = T::default();
        let net_state = false;
        let dir_io = NetDir::Inbound;
        let io = NetState::Idle(0);
        ZxNet { socket, event_ts, net_state, dir_io, io }
    }
}

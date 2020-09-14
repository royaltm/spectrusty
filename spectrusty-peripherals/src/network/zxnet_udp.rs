/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::slice;
use std::io::{self, Read};
use std::net::{UdpSocket, ToSocketAddrs, SocketAddr, IpAddr, Ipv4Addr};
use std::time::{Instant, Duration};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use super::zxnet::{HEAD_SIZE, ZxNetSocket};

/// Implements [ZxNetSocket] sending ZX-NET packets using UDP datagrams in real time.
///
/// Each ZX-NET data packet is being send as a single datagram. When a packet is being accepted
/// a special datagram is being sent back that contains only head part of the ZX-NET packet and
/// a flag indicating that this is the reply packet.
///
/// Duplicate spam messages are being removed before processing incoming data.
/// Packets that was recently replied to are being auto-replied when incoming again.
///
/// Requires an UDP socket to be "connected" to the remote party in order to send and receive data.
///
/// The original ZX-NET data packet is being prepended by 6 bytes of which 5 are the tag: **"ZXNET"**
/// and a flag byte if 0 indicating an incoming packet and 1 a reply packet.
#[derive(Debug)]
pub struct ZxNetUdpSyncSocket {
    sock: UdpSocket,
    packet: io::Cursor<Vec<u8>>,
    last_accepted: [u8;ACCEPT_SIZE],
    accepted_time: Instant,
}

impl ZxNetUdpSyncSocket {
    /// Binds the UDP socket to the indicated local address.
    pub fn bind<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<()> {
        self.sock = UdpSocket::bind(addr)?;
        self.setup_socket()?;
        Ok(())
    }
    /// Connects the UDP socket to the indicated remote address.
    pub fn connect<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<()> {
        self.sock.connect(addr)
    }

    fn setup_socket(&mut self) -> io::Result<()> {
        self.sock.set_read_timeout(Some(READ_ACCEPT_TIMEOUT))?;
        self.sock.set_nonblocking(true)?;
        self.sock.set_broadcast(true)
    }
}

const PACKET_TAG: &[u8] = b"ZXNET";
const KIND_NEW_DATA: u8 = 0;
const KIND_ACCEPTED: u8 = 1;
const KIND_INDEX: usize = 5;
const DATA_INDEX: usize = 6;
const ACCEPT_SIZE: usize = DATA_INDEX + HEAD_SIZE;
const MIN_SIZE: usize = ACCEPT_SIZE + 1;
const MAX_SIZE: usize = ACCEPT_SIZE + u8::max_value() as usize;
// The emulation will be paused for this long to get the response packet.
const READ_ACCEPT_TIMEOUT: Duration = Duration::from_millis(50);
// After this time the last accepted header will be ignored.
const LAST_ACCEPTED_TTL: Duration = Duration::from_secs(5);
/*
    packet > 
    accept < 
     TAG: "ZXNET"
    KIND: 0 (packet), 1 (accept)
    HEAD: 8 bytes
    BODY: ..
*/
impl ZxNetSocket for ZxNetUdpSyncSocket {
    fn packet_data(&self) -> &[u8] {
        &self.packet.get_ref()[DATA_INDEX..]
    }

    fn begin_packet(&mut self) {
        let vec = self.packet.get_mut();
        vec.resize(DATA_INDEX, 0);
        vec[0..5].copy_from_slice(PACKET_TAG);
        vec[KIND_INDEX] = KIND_NEW_DATA; // packet
        self.packet.set_position(DATA_INDEX as u64);
    }

    fn push_byte(&mut self, byte: u8) -> usize {
        let vec = self.packet.get_mut();
        vec.push(byte);
        vec.len() - DATA_INDEX
    }

    fn send_packet(&mut self) {
        match self.sock.send(self.packet.get_ref()) {
            Ok(len) => {
                trace!("sent: {} size: {}", len, len - DATA_INDEX);
            },
            Err(e) => {
                debug!("err send: {}", e);
            }
        }
    }

    fn recv_accept(&mut self) -> bool {
        let packet = self.packet.get_ref();
        if packet.len() > ACCEPT_SIZE {
            let mut buf = [0u8; ACCEPT_SIZE];
            while let Some(ACCEPT_SIZE) = Self::try_recv(&self.sock, &mut buf, true) {
                if buf[KIND_INDEX] == KIND_ACCEPTED && 
                   buf[DATA_INDEX..] == self.packet.get_ref()[DATA_INDEX..ACCEPT_SIZE] {
                    return true
                }
            }
        }
        false
    }

    fn outbound_index(&self) -> usize {
        self.packet.get_ref().len() - DATA_INDEX
    }

    fn recv_packet(&mut self) -> bool {
        self.packet.set_position(DATA_INDEX as u64);
        self.packet.get_mut().resize(MAX_SIZE, 0);
        while let Some(len @ MIN_SIZE..=MAX_SIZE) = Self::try_recv(&self.sock, self.packet.get_mut(), false) {
            let packet = self.packet.get_mut();
            packet.truncate(len);
            let last_accepted = &self.last_accepted;
            if last_accepted[KIND_INDEX] == KIND_ACCEPTED && 
               last_accepted[DATA_INDEX..ACCEPT_SIZE] == packet[DATA_INDEX..ACCEPT_SIZE] &&
               self.accepted_time.elapsed() < LAST_ACCEPTED_TTL {
                // re-send acceptance of the last accepted packet
                self.send_last_accepted();
                continue
            }
            return true
        }
        false
    }

    fn pull_byte(&mut self) -> Option<u8> {
        let mut byte = 0u8;
        if let Ok(1) = self.packet.read(slice::from_mut(&mut byte)) {
            return Some(byte)
        }
        None
    }

    fn inbound_index(&self) -> usize {
        self.packet.position() as usize - DATA_INDEX
    }

    fn send_accept(&mut self) {
        self.last_accepted.copy_from_slice(&self.packet.get_ref()[..ACCEPT_SIZE]);
        self.last_accepted[KIND_INDEX] = KIND_ACCEPTED; // accept
        self.send_last_accepted();
        self.accepted_time = Instant::now()
    }
}

impl ZxNetUdpSyncSocket {
    fn send_last_accepted(&mut self) {
        let res = self.sock.send(&self.last_accepted);
        match res {
            Ok(ACCEPT_SIZE) => {}
            Ok(len) => {
                debug!("wrong send bytes: {}", len);
            }
            Err(e) => {
                debug!("err send last accepted: {:?} {:?}", e.kind(), e);
            }
        }
    }

    fn try_recv(socket: &UdpSocket, buf: &mut [u8], blocking: bool) -> Option<usize> {
        if blocking {
            socket.set_nonblocking(false).expect("recv blocking failed (1)");
        }
        let ok = match socket.recv(buf) {
            Ok(len) if len >= ACCEPT_SIZE => Some(len),
            Ok(len) => {
                debug!("too short packet: {}", len);
                None
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => None,
            Err(e) if e.kind() == io::ErrorKind::TimedOut => None,
            Err(e) => {
                debug!("err recv: {:?} {:?}", e.kind(), e);
                None
            }
        };
        if blocking {
            socket.set_nonblocking(true).expect("recv blocking failed (2)");
        }
        ok.and_then(|len| {
            // println!("recv: {} {:?}", len, &buf[0..ACCEPT_SIZE]);
            if PACKET_TAG == &buf[0..5] {
                match buf[KIND_INDEX] {
                    0 => {
                        Some(len)
                    },
                    1 if len == ACCEPT_SIZE => Some(len),
                    _ => None
                }
            }
            else {
                debug!("bad packet: {} {:?}", len, buf);
                None
            }
        })
        .map(|len| {
            let mut bufpk = [0u8; MAX_SIZE];
            loop { // remove spam duplicates
                match socket.peek(&mut bufpk) {
                    Ok(plen) if plen == len && bufpk[0..len] == buf[0..len] => {}
                    _ => break len
                }
                socket.recv(&mut bufpk).unwrap();
            }
        })
    }
}

impl Default for ZxNetUdpSyncSocket {
    fn default() -> Self {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let packet = io::Cursor::new(Vec::with_capacity(MAX_SIZE));
        let last_accepted = Default::default();
        let accepted_time = Instant::now();
        let mut sock = ZxNetUdpSyncSocket {
            sock: UdpSocket::bind(addr).expect("can't create an UDP socket"),
            packet,
            last_accepted,
            accepted_time
        };
        sock.setup_socket().expect("setup socket failed");
        sock
    }
}

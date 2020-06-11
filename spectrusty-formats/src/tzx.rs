use core::num::NonZeroU32;
use core::convert::TryFrom;
use std::io::{self, Read, Write, Seek};

use super::tap::TapChunkWriter;

pub trait TzxChunk {
    type PulseIter: Iterator<Item=NonZeroU32>;
    fn id(&self) -> TzxId;
    fn len(&self) -> usize;
    fn pulse_iter(&self) -> Self::PulseIter;
    fn as_slice(&self) -> &[u8];
    fn write_to_tap<W: Write + Seek>(&self, rd: &mut TapChunkWriter<W>) -> io::Result<Option<usize>>;
}

macro_rules! tzx_id {
    ($($id:ident = $n:literal),*) => {
        #[repr(u8)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum TzxId {
            $($id = $n),*
        }

        impl TryFrom<u8> for TzxId {
            type Error = &'static str;
            fn try_from(id: u8) -> Result<Self, Self::Error> {
                match id {
                    $($n => Ok(TzxId::$id),)*
                    _ => Err("Unknown TZX ID")
                }
            }
        }

    };
}

tzx_id! {
    StandardSpeed    = 0x10,
    TurboSpeed       = 0x11,
    PureTone         = 0x12,
    SeqOfPulses      = 0x13,
    PureData         = 0x14,
    DirectRec        = 0x15,
    CswRecording     = 0x18,
    Generalized      = 0x19,
    Pause            = 0x20,
    GroupStart       = 0x21,
    GroupEnd         = 0x22,
    Jump             = 0x23,
    LoopStart        = 0x24,
    LoopEnd          = 0x25,
    CallSeq          = 0x26,
    Return           = 0x27,
    Select           = 0x28,
    StopIn48k        = 0x2A,
    SetLevel         = 0x2B,
    Text             = 0x30,
    Message          = 0x31,
    Archive          = 0x32,
    Hardware         = 0x33,
    Custom           = 0x35,
    Glue             = 0x5A
}

impl From<TzxId> for u8 {
    fn from(id: TzxId) -> u8 {
        id as u8
    }
}

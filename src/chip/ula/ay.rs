use core::ops::{Deref, DerefMut};
use crate::io::ay::{AyPortDecode, AyIoPort};
use crate::audio::{Blep, AmpLevels, sample::SampleDelta};
use crate::bus::{BusDevice, NullDevice};
use crate::chip::ula::{Ula, UlaVideoFrame};
use crate::clock::VideoTs;
use crate::memory::ZxMemory;

// pub trait AyAudioVBusDevice<E> {
//     fn render_ay_audio_vts<L: AmpLevels<B::SampleDelta>,
//                            V: VideoFrame>(&mut self, blep: &mut B, end_ts: VideoTs, chans: [usize; 3]);
// }

// impl<E, M, D, P, A, B, Z> AyAudioFrame<E> for Ula<M, D>
//     where M: ZxMemory,
//           D: BusDevice<Timestamp=VideoTs> +
//              Deref<Target=Ay3_891xBusDevice<VideoTs, P, A, B, Z>> +
//              DerefMut,
//           E: Blep,
//           // P: AyPortDecode,
//           // A: AyIoPort<Timestamp=VideoTs>,
//           // B: AyIoPort<Timestamp=VideoTs>,
// {
//     fn render_ay_audio_frame<V: AmpLevels<E::SampleDelta>>(&mut self, blep: &mut E, chans: [usize; 3]) {
//         self.bus.render_ay_audio::<V,UlaVideoFrame,E>(blep, self.tsc, chans)
//     }
// }

// impl<E, L, M, D, P, A, B> AyAudioFrame<E> for Ula<M, Ay3_891xBusDevice<VideoTs, P, A, B, D>>
//     where M: ZxMemory,
//           E: Blep<SampleDelta=L>,
//           L: SampleDelta,
//           // P: AyPortDecode,
//           // A: AyIoPort<Timestamp=VideoTs>,
//           // B: AyIoPort<Timestamp=VideoTs>,
//           // D: BusDevice<Timestamp=VideoTs>
// {
//     fn render_ay_audio_frame<V: AmpLevels<E::SampleDelta>>(&mut self, blep: &mut E, chans: [usize; 3]) {
//         self.bus.render_ay_audio::<V,UlaVideoFrame,E>(blep, self.tsc, chans)
//     }
// }

// impl<E, L, M> AyAudioFrame<E> for Ula<M, NullDevice<VideoTs>>
//     where M: ZxMemory,
//           E: Blep<SampleDelta=L>,
//           L: SampleDelta,
// {
//     fn render_ay_audio_frame<V: AmpLevels<L>>(&mut self, _blep: &mut E, _chans: [usize; 3]) {}
// }

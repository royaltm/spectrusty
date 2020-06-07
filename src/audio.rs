/*! # Audio API
```text
                     /---- ensure_audio_frame_time ----\
  +----------------------+                         +--------+
  |    UlaAudioFrame:    |  render_*_audio_frame   |        |
  |      AudioFrame +    | ======================> |  Blep  |
  | EarMicOutAudioFrame +|     end_audio_frame     |        |
  |   EarInAudioFrame +  |                         |        |
  |   [ AyAudioFrame ]   |                         +--------+
  +----------------------+                             |     
  |      AmpLevels       |        /----------------- (sum)   
  +----------------------+        |                    v       
                                  v                 carousel 
                           +-------------+   +--------------------+
                           | (WavWriter) |   | AudioFrameProducer |
                           +-------------+   +--------------------+
                                               || (AudioBuffer) ||
                                             +====================+
                                             |  * audio thread *  |
                                             |                    |
                                             | AudioFrameConsumer |
                                             +====================+
```
*/
pub use spectrusty_core::audio::*;
#[cfg(feature = "peripherals")] use crate::peripherals::ay::audio::AyAudioFrame;

// This is an arbitrary value for Blep implementation to reserve memory for additional samples.
// This is twice the value of the maximum number of wait-states added by an I/O device.
pub const MARGIN_TSTATES: FTs = 2800;

#[cfg(feature = "audio")]
pub use spectrusty_audio::*;

/// A grouping trait of common audio rendering traits for all emulated `Ula` chipsets.
#[cfg(feature = "peripherals")] pub trait UlaAudioFrame<B: Blep>: AudioFrame<B> +
                                  EarMicOutAudioFrame<B> +
                                  EarInAudioFrame<B> +
                                  AyAudioFrame<B> {}

#[cfg(not(feature = "peripherals"))] pub trait UlaAudioFrame<B: Blep>: AudioFrame<B> +
                                  EarMicOutAudioFrame<B> +
                                  EarInAudioFrame<B> {}

#[cfg(feature = "peripherals")]
impl<B: Blep, U> UlaAudioFrame<B> for U
    where U: AudioFrame<B> + EarMicOutAudioFrame<B> + EarInAudioFrame<B> + AyAudioFrame<B>
{}

#[cfg(not(feature = "peripherals"))]
impl<B: Blep, U> UlaAudioFrame<B> for U
    where U: AudioFrame<B> + EarMicOutAudioFrame<B> + EarInAudioFrame<B>
{}

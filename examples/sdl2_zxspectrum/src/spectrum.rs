use core::convert::TryFrom;
use core::fmt::{self, Write};
use std::path::Path;
use std::ffi::OsStr;
// use core::mem::{replace, ManuallyDrop};
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use std::io::{Read};

use sdl2::{Sdl};
use rand::prelude::*;

pub use zxspecemu::bus::*;
use zxspecemu::bus::ay::*;

use zxspecemu::bus::zxprinter::*;
use zxspecemu::bus::joystick::*;
use zxspecemu::bus::mouse::*;
pub use zxspecemu::clock::*;
pub use zxspecemu::z80emu::{Cpu, Z80NMOS};
pub use zxspecemu::memory::{ZxMemory, Memory48k, Memory16k};

use zxspecemu::audio::*;
use zxspecemu::audio::sample::AudioSample;
use zxspecemu::audio::carousel::AudioFrameResult;
use zxspecemu::audio::synth::*;
pub use zxspecemu::audio::ay::*;
pub use zxspecemu::chip::*;
pub use zxspecemu::chip::ula::*;
use zxspecemu::video::*;
use zxspecemu::formats::{
    sna
};
use zxspecemu::utils::tap::TapFileCabinet;

use super::audio::Audio;
use super::printer::ImageSpooler;
use super::peripherals::*;

pub use zxspecemu::video::{BorderSize, PixelBufRGB24};

const ROM48: &[u8] = include_bytes!("../../../resources/48k.rom");

pub type ZXPrinterToImage = ZxPrinter<UlaVideoFrame, ImageSpooler>;
pub type ZXSpectrum16 = ZXSpectrum<Z80NMOS, Memory16k, OptionalBusDevice<MultiJoystickBusDevice>>;
pub type ZXSpectrum48 = ZXSpectrum<Z80NMOS, Memory48k, OptionalBusDevice<MultiJoystickBusDevice>>;
pub type ZXSpectrum48DynBus = ZXSpectrum<Z80NMOS, Memory48k,
                                OptionalBusDevice<MultiJoystickBusDevice,
                                    DynamicBusDevice
                                >>;
pub type ZXBlep = BlepAmpFilter<BlepStereo<BandLimited<f32>>>;

#[derive(Clone, Copy, Debug, Default)]
pub struct BusDeviceIndexes {
    mouse: Option<usize>,
    printer: Option<usize>,
    ay_melodik: Option<usize>,
    ay_fullerbox: Option<usize>,
}

pub struct ZXSpectrum<C, M, B> {
    cpu: C,
    pub ula: Ula<M, B>,
    pub audio: Audio,
    bandlim: ZXBlep,
    // writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
    time_sync: ThreadSyncTimer,
    pub audible_tap: bool,
    pub tap_cabinet: TapFileCabinet,
    pub joystick_index: usize,
    bus_index: BusDeviceIndexes,
    pub mouse_rel: (i32, i32),
    nmi: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum FileType {
    Tap,
    Sna,
    Scr,
    Unknown
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            FileType::Tap => "TAP",
            FileType::Sna => "SNA",
            FileType::Scr => "SCR",
            FileType::Unknown => "Unknown",
        })
    }
}

// impl<M: ZxMemory> Drop for ZXSpectrum<M> {
//     fn drop(&mut self) {
//         // unsafe { ManuallyDrop::take(&mut self.device) }.close_audio_device()
//         unsafe { ManuallyDrop::into_inner(core::ptr::read(&mut self.audio)) }.close_audio_device()
//     }
// }
impl<C, M, B> ZXSpectrum<C, M, B>
    where M: ZxMemory + Default,
          B: BusDevice<Timestamp=VideoTs>,
          C: Cpu + std::fmt::Debug,
          Ula<M, B>: Default + AyAudioFrame<ZXBlep>
{
    pub fn create(sdl_context: &Sdl, latency: usize) -> Result<Self, Box<dyn std::error::Error>> {
        // let (tx, audio_rx) = sync_channel::<AudioBuffer>(3);
        // let audio = sdl2_subs::setup(sdl_context, Self::FRAME_TIME, tx).unwrap();
        // let audio_buffer = audio_rx.recv().unwrap();
        // audio.tx.send(audio_buffer.clone()).unwrap();
        // audio.tx.send(audio_buffer.clone()).unwrap();
        // let spec = hound::WavSpec {
        //     channels: 2,
        //     sample_rate: 44100,
        //     bits_per_sample: 32,
        //     sample_format: hound::SampleFormat::Float,
        // };
        // let writer = Some(hound::WavWriter::create("spectrum.wav", spec).unwrap());
        let ula = Ula::<M,B>::default();
        let tap_cabinet = TapFileCabinet::new();
        let audio = Audio::create(sdl_context, latency, ula.frame_duration_nanos())?;
        let mut bandlim = BlepAmpFilter::build(0.5)(BlepStereo::build(0.8)(BandLimited::<f32>::new(2)));
        ula.ensure_audio_frame_time(&mut bandlim, audio.sample_rate);
        let time_sync = ThreadSyncTimer::new(ula.frame_duration_nanos());
        let mut zx = ZXSpectrum {
            cpu: C::default(),
            ula,
            audio,
            bandlim,
            nmi: false,
            audible_tap: true,
            joystick_index: 0,
            tap_cabinet,
            // audio: ManuallyDrop::new(audio),
            bus_index: Default::default(),
            mouse_rel: (0, 0),
            time_sync
        };
        zx.ula.memory.load_into_rom(ROM48).unwrap();
        // Produce some noise in memory for nice visuals.
        zx.ula.memory.fill_mem(0x4000.., random).unwrap();
        zx.audio.resume();
        Ok(zx)
    }

    pub fn screen_size(border_size: BorderSize) -> (u32, u32) {
        UlaVideoFrame::screen_size_pixels(border_size)
    }

    pub fn reset(&mut self) {
        self.ula.reset(&mut self.cpu, true);
    }

    pub fn render_audio(&mut self) -> AudioFrameResult<()> {
        let Self { ula, bandlim, audio, .. } = self;
        ula.render_ay_audio_frame::<AyAmps<f32>>(bandlim, [0, 1, 2]);
        // ula.render_ay_audio_frame::<AyAmps<f32>>(bandlim, [2, 2, 2]); // mono
        if self.audible_tap {
            ula.render_earmic_out_audio_frame::<EarMicAmps4<f32>>(bandlim, 2);
            ula.render_ear_in_audio_frame::<EarInAmps2<f32>>(bandlim, 2);
        }
        else {
            ula.render_earmic_out_audio_frame::<EarOutAmps4<f32>>(bandlim, 2);
        }
        // close current frame
        let frame_sample_count = ula.end_audio_frame(bandlim);
        // render BLEP frame into the sample buffer
        let output_channels = audio.channels.into();
        audio.producer.render_frame(|ref mut vec| {
            let sample_iter = bandlim.sum_iter::<f32>(0)
                             .zip(bandlim.sum_iter::<f32>(1))
                             .map(|(a,b)| [a,b]);
            vec.resize(frame_sample_count * output_channels, f32::silence());
            // render each sample
            for (chans, samples) in vec.chunks_mut(output_channels).zip(sample_iter) {
                // write to the wav file
                // writer.write_sample(f32::from_sample(sample)).unwrap();
                // convert sample type
                // let sample = T::from_sample(samples);
                // write sample to each channel
                for (p, sample) in chans.iter_mut().zip(samples.iter()) {
                    *p = *sample;
                }
            }
        });
        // prepare BLEP for the next frame
        bandlim.next_frame();
        // send sample buffer to the consumer
        audio.producer.send_frame()
    }

    #[inline]
    pub fn run_single_frame(&mut self) -> FTs {
        if self.nmi {
            self.trigger_nmi();
        }
        if let Some(ref mut writer) = self.tap_cabinet.mic_out_pulse_writer() {
            let mut pulses_iter = self.ula.mic_out_iter_pulses();
            // .map(|pulse| {
            //     println!("{}", pulse.get());
            //     pulse
            // });
            let chunks = writer.write_pulses_as_tap_chunks(&mut pulses_iter).unwrap();
            if chunks != 0 {
                info!("Saved: {} chunks", chunks);
            }
        }

        let fts = self.ula.ensure_next_frame().as_tstates();
        let mut should_stop = false;

        if let Some(ref mut feeder) = self.tap_cabinet.ear_in_pulse_iter() {
            let mut feeder = feeder.peekable();
            if feeder.peek().is_some() {
                self.ula.feed_ear_in(&mut feeder, Some(1));
            }
            else {
                should_stop = true;
            }
        }
        self.ula.execute_next_frame(&mut self.cpu);
        // eprintln!("{:?}", self.ula);
        // eprintln!("{:?}", self.cpu);
        // let _ = io::stdin().lock().lines().next().unwrap();
        if should_stop {
            info!("Auto STOP: End of TAPE");
            self.tap_cabinet.stop();
            // self.tap_cabinet.rewind_tap()?;
        }
        self.ula.current_tstate() - fts
    }

    pub fn run_frames_accelerated(&mut self) -> FTs {
        let mut sum: FTs = 0;
        while self.time_sync.check_frame_elapsed().is_none() {
            sum += self.run_single_frame();
        }
        sum
    }

    pub fn synchronize_thread_to_frame(&mut self) -> bool {
        if let Err(missed) = self.time_sync.synchronize_thread_to_frame() {
            debug!("*** missed frames: {} ***", missed);
            return false
        }
        true
    }

    #[inline(always)]
    pub fn render_pixels<P: PixelBuffer>(&mut self, buffer: &mut [u8], pitch: usize, border_size: BorderSize) {
        // eprintln!("before {:?}", self.ula);
        self.ula.render_video_frame::<P>(buffer, pitch, border_size);
        // eprintln!("after {:?}", self.ula);
    }

    pub fn trigger_nmi(&mut self) {
        if self.ula.nmi(&mut self.cpu) {
            self.nmi = false;
        }
        else {
            self.nmi = true;
        }
    }

    pub fn device_info(&self) -> String
        where Self: JoystickAccess + DynBusAccess
    {
        let mut info = format!("{}kb", (M::RAMTOP as u32 + 1)/1024-16);
        if let Some(joy) = self.joystick_ref() {
            if self.joystick_index != 0 {
                write!(info, " + {} #{} Joy.", joy, self.joystick_index + 1).unwrap();
            }
            else {
                write!(info, " + {} Joystick", joy).unwrap();
            }
        }
        if let Some(devices) = self.dynbus_devices_ref() {
            let pr_index = self.bus_index.printer;
            for (i, dev) in devices.into_iter().enumerate() {
                write!(info, " + {}", dev).unwrap();
                if pr_index.filter(|&pri| pri == i).is_some() {
                    let spooler: &ImageSpooler = dev.downcast_ref::<ZXPrinterToImage>().unwrap();
                    let lines = spooler.lines_buffered();
                    if lines != 0 || spooler.is_spooling() {
                        write!(info, "[{}]", lines).unwrap();
                    }
                }
            }
        }
        info
    }

    pub fn print_current_tap_chunk(&self, no: u32) {
        println!("Current TAP file: #{} {} / chunk: {}",
            self.tap_cabinet.current_tap_index(),
            self.tap_cabinet.current_tap_file_name().unwrap_or("".into()),
            no
        );
    }

    pub fn print_current_tap(&self) {
        println!("Current TAP file: #{} {}",
            self.tap_cabinet.current_tap_index(),
            self.tap_cabinet.current_tap_file_name().unwrap_or("".into())
        );
    }

    pub fn handle_file<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<FileType> {
        match path.as_ref().extension().and_then(OsStr::to_str) {
            Some(s) if s.eq_ignore_ascii_case("tap") => {
                self.tap_cabinet.add_file(path)?;
                self.tap_cabinet.select_last_tap();
                self.print_current_tap();
                Ok(FileType::Tap)
            }
            Some(s) if s.eq_ignore_ascii_case("scr") => {
                let file = std::fs::File::open(path)?;
                self.load_scr(file)?;
                Ok(FileType::Scr)
            }
            Some(s) if s.eq_ignore_ascii_case("sna") => {
                let file = std::fs::File::open(path)?;
                self.load_sna(file)?;
                Ok(FileType::Sna)
            }
            _ => {
                Ok(FileType::Unknown)
            }
        }
    }

    pub fn load_scr<R: Read>(&mut self, mut rd: R) -> std::io::Result<()> {
        rd.read_exact(self.ula.memory.screen_mut(0)?)
    }

    pub fn load_sna<R: Read>(&mut self, rd: R) -> std::io::Result<()> {
        let border = sna::read_sna(rd, &mut self.cpu, &mut self.ula.memory)?;
        self.ula.set_border_color(border);
        // eprintln!("{:?}", self.cpu);
        // eprintln!("{:?}", self.ula);
        Ok(())
    }
}

impl ZXSpectrum48DynBus {
    fn dynbus_mut(&mut self) -> &mut DynamicBusDevice {
        self.ula.bus_device_mut().next_device_mut()
    }
    fn dynbus_ref(&self) -> &DynamicBusDevice {
        self.ula.bus_device_ref().next_device_ref()
    }
    pub fn remove_all_devices(&mut self) {
        self.bus_index = Default::default();
        self.dynbus_mut().clear();
    }
    pub fn add_mouse(&mut self) {
        if self.bus_index.mouse.is_none() {
            self.bus_index.mouse = Some(self.dynbus_mut().append_device(KempstonMouse::default()));
        }
    }
    pub fn add_ay_melodik(&mut self) {
        if self.bus_index.ay_melodik.is_none() {
            let ay: Ay3_891xMelodik = Default::default();
            self.bus_index.ay_melodik = Some(self.dynbus_mut().append_device(ay));
        }
    }
    pub fn add_fullerbox(&mut self) {
        if self.bus_index.ay_fullerbox.is_none() {
            let ay: Ay3_891xFullerBox = Default::default();
            self.bus_index.ay_fullerbox = Some(self.dynbus_mut().append_device(ay));
            *self.joystick_device_mut() = Some(
                MultiJoystickBusDevice::new_with(
                    JoystickSelect::try_from("Fuller").unwrap()
            ))
        }
    }
    pub fn add_printer(&mut self) {
        if self.bus_index.printer.is_none() {
            self.bus_index.printer = Some(self.dynbus_mut()
                .append_device(ZXPrinterToImage::default()));
        }
    }
}

impl DynBusAccess for ZXSpectrum16 {}
impl DynBusAccess for ZXSpectrum48 {}
impl DynBusAccess for ZXSpectrum48DynBus {
    fn dynbus_devices_mut(&mut self) -> Option<&mut DynamicBusDevice> {
        Some(self.dynbus_mut())
    }
    fn dynbus_devices_ref(&self) -> Option<&DynamicBusDevice> {
        Some(self.dynbus_ref())
    }
}

impl MouseAccess for ZXSpectrum16 {}
impl MouseAccess for ZXSpectrum48 {}
impl MouseAccess for ZXSpectrum48DynBus {
    fn mouse_mut(&mut self) -> Option<&mut KempstonMouseDevice> {
        self.bus_index.mouse.map(move |index| {
            &mut **self.dynbus_mut().as_device_mut::<KempstonMouse>(index)
        })
    }    
}
impl SpoolerAccess for ZXSpectrum16 {}
impl SpoolerAccess for ZXSpectrum48 {}
impl SpoolerAccess for ZXSpectrum48DynBus {
    fn spooler_mut(&mut self) -> Option<&mut ImageSpooler> {
        self.bus_index.printer.map(move |index| {
            &mut ***self.dynbus_mut().as_device_mut::<ZXPrinterToImage>(index)
        })
    }
    fn spooler_ref(&self) -> Option<&ImageSpooler> {
        self.bus_index.printer.map(|index| {
            &***self.dynbus_ref().as_device_ref::<ZXPrinterToImage>(index)
        })
    }
}

impl JoystickAccess for ZXSpectrum16 {
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref()
    }
}
impl JoystickAccess for ZXSpectrum48 {
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref()
    }
}
impl JoystickAccess for ZXSpectrum48DynBus {
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref()
    }
}

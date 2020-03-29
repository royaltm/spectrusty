use core::convert::TryFrom;
use core::fmt;
// use core::mem::{replace, ManuallyDrop};
use std::io::{Read};
use std::path::Path;
use std::fs;
use std::ffi::OsStr;

pub mod audio;
pub mod printer;
pub mod peripherals;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use sdl2::{Sdl};
use rand::prelude::*;

use spectrusty::audio::AudioSample;
use spectrusty::audio::carousel::AudioFrameResult;
use spectrusty::audio::synth::*;
use spectrusty::bus::ay::*;
use spectrusty::bus::joystick::*;
use spectrusty::bus::mouse::*;
use spectrusty::formats::{
    sna
};
use spectrusty::utils::tap::TapFileCabinet;
use spectrusty::video::*;

use audio::Audio;
pub use printer::*;
use peripherals::*;

pub use spectrusty::peripherals::ay::audio::*;
pub use spectrusty::peripherals::{KeyboardInterface, serial::SerialKeypad};
pub use spectrusty::peripherals::memory::ZxInterface1MemExt;
pub use spectrusty::audio::*;
pub use spectrusty::bus::*;
pub use spectrusty::chip::{*, ula::*, ula128::*};
pub use spectrusty::clock::*;
pub use spectrusty::memory::{ZxMemory, Memory48kEx, Memory48k, Memory16k};
pub use spectrusty::video::{BorderSize, PixelBufRGB24, Video};
pub use spectrusty::z80emu::{Cpu, Z80NMOS};

const IF1_ROM_PATH: &str = "../../resources/if1-2.rom";
const ROM48: &[u8] = include_bytes!("../../../resources/48.rom");
const ROM128_0: &[u8] = include_bytes!("../../../resources/128-0.rom");
const ROM128_1: &[u8] = include_bytes!("../../../resources/128-1.rom");

pub type ZXSpectrum16 = ZXSpectrum<Z80NMOS, Ula<Memory16k, OptJoystickBusDevice>>;
pub type ZXSpectrum48 = ZXSpectrum<Z80NMOS, Ula<Memory48k, OptJoystickBusDevice>>;
pub type ZXSpectrum128 = ZXSpectrum<Z80NMOS, Ula128<Ay3_8912>>;
pub type ZXSpectrum128If1 = ZXSpectrum<Z80NMOS, Ula128<ZxInterface1<Ula128VidFrame, Ay3_8912>, ZxInterface1MemExt>>;
pub type ZXSpectrum48DynBus = ZXSpectrum<Z80NMOS,
                                        Ula<Memory48k,
                                            OptJoystickBusDevice<
                                                DynamicBusDevice
                                        >>>;
pub type ZXSpectrum48If1DynBus = ZXSpectrum<Z80NMOS,
                                        Ula<Memory48kEx,
                                            ZxInterface1<UlaVideoFrame,
                                                OptJoystickBusDevice<
                                                    DynamicBusDevice
                                            >>,
                                            ZxInterface1MemExt
                                        >>;
pub type ZXBlep = BlepAmpFilter<BlepStereo<BandLimited<f32>>>;

#[derive(Clone, Copy, Debug, Default)]
pub struct BusDeviceIndexes {
    mouse: Option<usize>,
    printer: Option<usize>,
    ay_melodik: Option<usize>,
    ay_fullerbox: Option<usize>,
}

pub struct ZXSpectrum<C, U> {
    pub cpu: C,
    pub ula: U,
    pub audio: Audio,
    bandlim: ZXBlep,
    // writer: Option<hound::WavWriter<std::io::BufWriter<fs::File>>>,
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
impl<C, U> ZXSpectrum<C, U>
    where C: Cpu + std::fmt::Debug,
          U: Default + UlaCommon + UlaAudioFrame<ZXBlep>
{
    pub fn create(sdl_context: &Sdl, latency: usize) -> Result<Self, Box<dyn std::error::Error>> {
        // let spec = hound::WavSpec {
        //     channels: 2,
        //     sample_rate: 44100,
        //     bits_per_sample: 32,
        //     sample_format: hound::SampleFormat::Float,
        // };
        // let writer = Some(hound::WavWriter::create("spectrum.wav", spec).unwrap());
        let ula = U::default();
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
        if U::Memory::ROM_SIZE == ROM48.len() {
            zx.ula.memory_mut().load_into_rom(ROM48).unwrap();
        }
        else {
            zx.ula.memory_mut().load_into_rom_bank(0, ROM128_0).unwrap();
            zx.ula.memory_mut().load_into_rom_bank(1, ROM128_1).unwrap();
        }
        // Produce some noise in memory for nice visuals.
        zx.ula.memory_mut().fill_mem(0x4000.., random).unwrap();
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
            let pulses_iter = self.ula.mic_out_pulse_iter();
            // .map(|pulse| {
            //     println!("{}", pulse.get());
            //     pulse
            // });
            let chunks = writer.write_pulses_as_tap_chunks(pulses_iter).unwrap();
            if chunks != 0 {
                info!("Saved: {} chunks", chunks);
            }
        }

        self.ula.ensure_next_frame();
        let fts = self.ula.current_tstate();
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
                let file = fs::File::open(path)?;
                self.load_scr(file)?;
                Ok(FileType::Scr)
            }
            Some(s) if s.eq_ignore_ascii_case("sna") => {
                let file = fs::File::open(path)?;
                self.load_sna(file)?;
                Ok(FileType::Sna)
            }
            _ => {
                Ok(FileType::Unknown)
            }
        }
    }

    pub fn load_scr<R: Read>(&mut self, mut rd: R) -> std::io::Result<()> {
        rd.read_exact(self.ula.memory_mut().screen_mut(0)?)
    }

    pub fn load_sna<R: Read>(&mut self, rd: R) -> std::io::Result<()> {
        let border = sna::read_sna(rd, &mut self.cpu, self.ula.memory_mut())?;
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
}

impl ZXSpectrum48If1DynBus {
    pub fn load_if1_rom(&mut self) -> std::io::Result<()> {
        let file = fs::File::open(IF1_ROM_PATH)?;
        self.ula.memory_ext_mut().load_if1_rom(file)
    }
    fn dynbus_mut(&mut self) -> &mut DynamicBusDevice {
        self.ula.bus_device_mut().next_device_mut().next_device_mut()
    }
    fn dynbus_ref(&self) -> &DynamicBusDevice {
        self.ula.bus_device_ref().next_device_ref().next_device_ref()
    }
}

impl ZXSpectrum128If1 {
    pub fn load_if1_rom(&mut self) -> std::io::Result<()> {
        let file = fs::File::open(IF1_ROM_PATH)?;
        self.ula.memory_ext_mut().load_if1_rom(file)
    }
}

impl<C, U> DynamicDeviceAccess for ZXSpectrum<C, U>
    where Self: DeviceAccess<U::VideoFrame>, U: Video//, U::VideoFrame: VideoFrame
    // where C: Cpu + std::fmt::Debug,
          // U: Default + UlaCommon + UlaAudioFrame<ZXBlep>
{
    fn remove_all_devices(&mut self) {
        self.bus_index = Default::default();
        if let Some(dynbus) = self.dynbus_devices_mut() {
            dynbus.clear();
        }
    }
    fn add_mouse(&mut self) {
        if self.bus_index.mouse.is_none() {
            if let Some(dynbus) = self.dynbus_devices_mut() {
                self.bus_index.mouse = Some(dynbus.append_device(KempstonMouse::default()));
            }
        }
    }
    fn add_ay_melodik(&mut self) {
        if self.bus_index.ay_melodik.is_none() {
            if let Some(dynbus) = self.dynbus_devices_mut() {
                let ay: Ay3_891xMelodik = Default::default();
                self.bus_index.ay_melodik = Some(dynbus.append_device(ay));
            }
        }
    }
    fn add_fullerbox(&mut self) {
        if self.bus_index.ay_fullerbox.is_none() {
            if let Some(dynbus) = self.dynbus_devices_mut() {
                let ay: Ay3_891xFullerBox = Default::default();
                self.bus_index.ay_fullerbox = Some(dynbus.append_device(ay));
                *self.joystick_device_mut() = Some(
                    MultiJoystickBusDevice::new_with(
                        JoystickSelect::try_from("Fuller").unwrap()
                ))
            }
        }
    }
    fn add_printer(&mut self) {
        if self.bus_index.printer.is_none() {
            if let Some(dynbus) = self.dynbus_devices_mut() {
                self.bus_index.printer = Some(dynbus
                    .append_device(ZXPrinterToImage::<UlaVideoFrame>::default()));
            }
        }
    }
}

impl DeviceAccess<UlaVideoFrame> for ZXSpectrum16 {
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref()
    }
}

impl DeviceAccess<UlaVideoFrame> for ZXSpectrum48 {
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref()
    }
}

impl DeviceAccess<Ula128VidFrame> for ZXSpectrum128 {
    fn gfx_printer_mut(&mut self) -> Option<&mut dyn ZxGfxPrinter> {
        Some(&mut self.ula.bus_device_mut().ay_io
                                           .port_a
                                           .serial2
                                           .writer
                                           .grabber)
    }
    fn gfx_printer_ref(&self) -> Option<&dyn ZxGfxPrinter> {
        Some(&self.ula.bus_device_ref().ay_io
                                       .port_a
                                       .serial2
                                       .writer
                                       .grabber)
    }
    fn keypad_mut(&mut self) -> Option<&mut SerialKeypad<Ula128VidFrame>> {
        Some(&mut self.ula.bus_device_mut().ay_io.port_a.serial1)
    }
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut().next_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref().next_device_ref()
    }
    fn static_ay3_8912_ref(&self) -> Option<&Ay3_8912> {
        Some(&self.ula.bus_device_ref())
    }
}

impl DeviceAccess<Ula128VidFrame> for ZXSpectrum128If1 {
    fn microdrives_ref(&self) -> Option<&ZXMicrodrives<Ula128VidFrame>> {
        Some(&self.ula.bus_device_ref().microdrives)
    }
    fn microdrives_mut(&mut self) -> Option<&mut ZXMicrodrives<Ula128VidFrame>> {
        Some(&mut self.ula.bus_device_mut().microdrives)
    }
    fn network_ref(&self) -> Option<&ZxNetUdpSyncSocket> {
        Some(&self.ula.bus_device_ref().network.socket)
    }
    fn network_mut(&mut self) -> Option<&mut ZxNetUdpSyncSocket> {
        Some(&mut self.ula.bus_device_mut().network.socket)
    }
    fn gfx_printer_mut(&mut self) -> Option<&mut dyn ZxGfxPrinter> {
        Some(&mut self.ula.bus_device_mut().next_device_mut().ay_io
                                                             .port_a
                                                             .serial2
                                                             .writer
                                                             .grabber)
    }
    fn gfx_printer_ref(&self) -> Option<&dyn ZxGfxPrinter> {
        Some(&self.ula.bus_device_ref().next_device_ref().ay_io
                                                         .port_a
                                                         .serial2
                                                         .writer
                                                         .grabber)
    }
    fn keypad_mut(&mut self) -> Option<&mut SerialKeypad<Ula128VidFrame>> {
        Some(&mut self.ula.bus_device_mut().next_device_mut().ay_io.port_a.serial1)
    }
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut().next_device_mut().next_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref().next_device_ref().next_device_ref()
    }
    fn static_ay3_8912_ref(&self) -> Option<&Ay3_8912> {
        Some(&self.ula.bus_device_ref().next_device_ref())
    }
}

impl DeviceAccess<UlaVideoFrame> for ZXSpectrum48DynBus {
    fn spooler_mut(&mut self) -> Option<&mut ImageSpooler> {
        self.bus_index.printer.map(move |index| {
            &mut ***self.dynbus_mut().as_device_mut::<ZXPrinterToImage<UlaVideoFrame>>(index)
        })
    }
    fn spooler_ref(&self) -> Option<&ImageSpooler> {
        self.bus_index.printer.map(|index| {
            &***self.dynbus_ref().as_device_ref::<ZXPrinterToImage<UlaVideoFrame>>(index)
        })
    }
    fn dynbus_devices_mut(&mut self) -> Option<&mut DynamicBusDevice> {
        Some(self.dynbus_mut())
    }
    fn dynbus_devices_ref(&self) -> Option<&DynamicBusDevice> {
        Some(self.dynbus_ref())
    }
    fn mouse_mut(&mut self) -> Option<&mut KempstonMouseDevice> {
        self.bus_index.mouse.map(move |index| {
            &mut **self.dynbus_mut().as_device_mut::<KempstonMouse>(index)
        })
    }
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref()
    }
}

impl DeviceAccess<UlaVideoFrame> for ZXSpectrum48If1DynBus {
    fn microdrives_ref(&self) -> Option<&ZXMicrodrives<UlaVideoFrame>> {
        Some(&self.ula.bus_device_ref().microdrives)
    }
    fn microdrives_mut(&mut self) -> Option<&mut ZXMicrodrives<UlaVideoFrame>> {
        Some(&mut self.ula.bus_device_mut().microdrives)
    }
    fn network_ref(&self) -> Option<&ZxNetUdpSyncSocket> {
        Some(&self.ula.bus_device_ref().network.socket)
    }
    fn network_mut(&mut self) -> Option<&mut ZxNetUdpSyncSocket> {
        Some(&mut self.ula.bus_device_mut().network.socket)
    }
    // fn gfx_printer_mut(&mut self) -> Option<&mut dyn ZxGfxPrinter> {
    //     Some(&mut self.ula.bus_device_mut().serial
    //                                        .writer
    //                                        .grabber)
    // }
    // fn gfx_printer_ref(&self) -> Option<&dyn ZxGfxPrinter> {
    //     Some(&self.ula.bus_device_ref().serial
    //                                    .writer
    //                                    .grabber)
    // }
    fn spooler_mut(&mut self) -> Option<&mut ImageSpooler> {
        self.bus_index.printer.map(move |index| {
            &mut ***self.dynbus_mut().as_device_mut::<ZXPrinterToImage<UlaVideoFrame>>(index)
        })
    }
    fn spooler_ref(&self) -> Option<&ImageSpooler> {
        self.bus_index.printer.map(|index| {
            &***self.dynbus_ref().as_device_ref::<ZXPrinterToImage<UlaVideoFrame>>(index)
        })
    }
    fn dynbus_devices_mut(&mut self) -> Option<&mut DynamicBusDevice> {
        Some(self.dynbus_mut())
    }
    fn dynbus_devices_ref(&self) -> Option<&DynamicBusDevice> {
        Some(self.dynbus_ref())
    }
    fn mouse_mut(&mut self) -> Option<&mut KempstonMouseDevice> {
        self.bus_index.mouse.map(move |index| {
            &mut **self.dynbus_mut().as_device_mut::<KempstonMouse>(index)
        })
    }
    fn joystick_device_mut(&mut self) -> &mut Option<MultiJoystickBusDevice> {
        self.ula.bus_device_mut().next_device_mut()
    }
    fn joystick_device_ref(&self) -> &Option<MultiJoystickBusDevice> {
        self.ula.bus_device_ref().next_device_ref()
    }
}

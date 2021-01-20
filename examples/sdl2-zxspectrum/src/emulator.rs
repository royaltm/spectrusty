/*
    sdl2-zxspectrum: ZX Spectrum emulator example as a SDL2 application.
    Copyright (C) 2020-2021  Rafal Michalski

    For the full copyright notice, see the main.rs file.
*/
use core::convert::TryFrom;
use core::fmt;
use core::ops::Range;
use std::path::Path;
use std::borrow::Cow;
use std::ffi::OsStr;
use std::io;
use std::fs;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ::serde::Serialize;
use arrayvec::ArrayString;
use sdl2::{mouse::MouseButton, keyboard::{Keycode, Mod as Modifier}};

use spectrusty::audio::{
    BlepAmpFilter, BlepStereo, AudioSample, AudioFrame,
    synth::BandLimited,
    carousel::AudioFrameResult
};
use spectrusty::z80emu::{Z80Any, Cpu};
use spectrusty::audio::{UlaAudioFrame, host::sdl2::AudioHandle};

use spectrusty::chip::{
    ThreadSyncTimer, UlaCommon, HostConfig, ControlUnit, MemoryAccess,
};

use spectrusty::bus::{
    zxinterface1::{
        MicroCartridge
    }
};
use spectrusty::formats::{
    tap::{TapChunkRead, TapReadInfoIter, TapChunkInfo},
    mdr::MicroCartridgeExt,
    scr::{LoadScr, ScreenDataProvider},
    snapshot::{SnapshotCreator, ensure_cpu_is_safe_for_snapshot},
    z80
};
use spectrusty::peripherals::{
    mouse::MouseButtons,
};
use spectrusty::video::{
    VideoFrame, Video
};
use zxspectrum_common::{
    JoystickAccess, DeviceAccess,
    ModelRequest,
    MouseAccess,
    UlaPlusMode
};
use spectrusty_utils::{
    keyboard::sdl2::{
        update_joystick_from_key_event,
        update_keymap_with_modifier,
        update_keypad_keys_with_modifier
    },
    tap::Tap,
    printer::{DotMatrixGfx}
};

mod nonblocking;
mod interface1;
mod printer;
mod serde;
mod snapshot;
use self::nonblocking::NonBlockingStdinReader;
use self::printer::EpsonGfxFilteredStdoutWriter;
pub use self::printer::{ZxPrinter, DynSpoolerAccess, SpoolerAccess};
pub use self::interface1::{ZxInterface1, ZxInterface1Access};
pub use snapshot::ZxSpectrumModelSnap;
pub use spectrusty::peripherals::memory::ZxInterface1MemExt;

pub type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

type Sample = f32;
type BlepDelta = f32;
pub type Audio = AudioHandle<Sample>;
pub type BandLim = BlepAmpFilter<BlepStereo<BandLimited<BlepDelta>>>;
pub type EmulatorState = zxspectrum_common::EmulatorState<fs::File>;
/// ZX Spectrum with TAPs as direct files.
pub type ZxSpectrum<C, U> = zxspectrum_common::ZxSpectrum<C, U, fs::File>;
pub type ZxSpectrumModel<C=Z80Any, X=ZxInterface1MemExt> = zxspectrum_common::ZxSpectrumModel<
                                        C,
                                        serde::SerdeDynDevice,
                                        X,
                                        fs::File,
                                        NonBlockingStdinReader,
                                        EpsonGfxFilteredStdoutWriter>;

/// This is the main emulator class.
#[derive(Serialize)]
pub struct ZxSpectrumEmu<'a, C: Cpu, U> {
    pub model: ModelRequest,
    #[serde(flatten)]
    pub spectrum: ZxSpectrum<C, U>,
    #[serde(skip)]
    pub audio: &'a mut Audio,
    #[serde(skip)]
    pub time_sync: ThreadSyncTimer,
    #[serde(skip)]
    bandlim: BandLim,
    #[serde(skip)]
    pub mouse_rel: (i32, i32),
    #[serde(skip)]
    info_text: String,
    #[serde(skip)]
    info_range: Range<usize>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    Json, Sna, Z80
}

pub fn snapshot_kind<P: AsRef<Path>>(filepath: P) -> Option<SnapshotKind> {
    match filepath.as_ref().extension().and_then(OsStr::to_str) {
        Some(ext) if ext.eq_ignore_ascii_case("json") => Some(SnapshotKind::Json),
        Some(ext) if ext.eq_ignore_ascii_case("sna") => Some(SnapshotKind::Sna),
        Some(ext) if ext.eq_ignore_ascii_case("z80") => Some(SnapshotKind::Z80),
        _ => None
    }
}

impl<'a, C: Cpu, U> ZxSpectrumEmu<'a, C, U> {
    pub fn new_with(
            model: ModelRequest,
            spectrum: ZxSpectrum<C, U>,
            audio: &'a mut Audio
        ) -> Result<Self>
        where U: HostConfig + AudioFrame<BandLim>
    {
        // let spec = hound::WavSpec {
        //     channels: 2,
        //     sample_rate: 44100,
        //     bits_per_sample: 32,
        //     sample_format: hound::SampleFormat::Float,
        // };
        // let writer = Some(hound::WavWriter::create("spectrum.wav", spec).unwrap());
        // let audio = Audio::create(sdl_context, U::frame_duration_nanos(), latency)?;
        let mut bandlim = BlepAmpFilter::build(0.25)(BlepStereo::build(0.86)(BandLimited::new(audio.channels.into())));
        spectrum.ula.ensure_audio_frame_time(&mut bandlim, audio.sample_rate, U::effective_cpu_rate(1.0));
        let time_sync = ThreadSyncTimer::new(U::frame_duration_nanos());
        Ok(ZxSpectrumEmu {
            model,
            spectrum,
            audio,
            time_sync,
            bandlim,
            mouse_rel: (0, 0),
            info_text: String::new(),
            info_range: 0..0
        })
    }

    pub fn resume_audio_if_producing(&self) {
        if !(self.spectrum.state.paused || self.spectrum.state.turbo) {
            self.audio.play();
        }
    }

    pub fn target_screen_size(&self) -> (u32, u32)
        where U: Video
    {
        let (w, h) = U::VideoFrame::screen_size_pixels(self.spectrum.state.border_size);
        (w * 2, h * 2)
    }

    pub fn synchronize_thread_to_frame(&mut self) -> bool {
        if let Err(missed) = self.time_sync.synchronize_thread_to_frame() {
            debug!("*** missed frames: {} ***", missed);
            return false
        }
        true
    }

    pub fn render_audio(&mut self) -> AudioFrameResult<()>
        where U: UlaCommon  + UlaAudioFrame<BandLim>
    {
        let frame_sample_count = self.spectrum.render_audio(&mut self.bandlim);
        let Self { audio, bandlim, .. } = self;
        let output_channels = audio.channels.into();
        audio.producer.render_frame(|ref mut vec| {
            let sample_iter = bandlim.sum_iter::<Sample>(0)
                             .zip(bandlim.sum_iter::<Sample>(1))
                             .map(|(a, b)| [a, b]);
            vec.resize(frame_sample_count * output_channels, Sample::silence());
            // render each sample
            for (chans, samples) in vec.chunks_mut(output_channels).zip(sample_iter) {
                // write to the wav file
                // writer.write_sample(Sample::from_sample(sample)).unwrap();
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

    pub fn move_mouse(&mut self, dx: i32, dy: i32) {
        self.mouse_rel.0 += dx;
        self.mouse_rel.1 += dy;
    }

    /// Send mouse positions at most once every frame to prevent overflowing.
    pub fn send_mouse_move(&mut self, viewport: (u32, u32))
        where U: Video,
              ZxSpectrum<C, U>: MouseAccess
    {
        let border = self.spectrum.state.border_size;
        match self.mouse_rel {
            (0, 0) => {},
            (dx, dy) => {
                if let Some(mouse) = self.spectrum.mouse_interface() {
                    let (sx, sy) = <U as Video>::VideoFrame::screen_size_pixels(border);
                    let (vx, vy) = viewport;
                    let dx = (dx * 2 * sx as i32 / vx as i32) as i16;
                    let dy = (dy * 2 * sy as i32 / vy as i32) as i16;
                    // println!("{}x{}", dx, dy);
                    mouse.move_mouse((dx, dy).into())
                }
                self.mouse_rel = (0, 0);
            }
        }
    }

    pub fn update_mouse_button(&mut self, button: MouseButton, pressed: bool)
        where ZxSpectrum<C, U>: MouseAccess
    {
        if let Some(mouse) = self.spectrum.mouse_interface() {
            let button_mask = match button {
                MouseButton::Left => MouseButtons::LEFT,
                MouseButton::Right => MouseButtons::RIGHT,
                _ => return
            };
            let buttons = mouse.get_buttons();
            mouse.set_buttons(if pressed {
                buttons|button_mask
            }
            else {
                buttons&!button_mask
            })
        }
    }

    pub fn handle_keypress_event(&mut self, key: Keycode, modifier: Modifier, pressed: bool)
        where U: UlaCommon + DeviceAccess,
              ZxSpectrum<C, U>: JoystickAccess
    {
        const FIRE_KEY: Keycode = Keycode::RCtrl;
        if !update_joystick_from_key_event(key, pressed, FIRE_KEY,
                                            || self.spectrum.joystick_interface()) {
            self.spectrum.update_keyboard(|keymap|
                update_keymap_with_modifier(keymap, key, pressed, modifier)
            );
            self.spectrum.update_keypad128_keys(|padmap|
                update_keypad_keys_with_modifier(padmap, key, pressed, modifier)
            );
        }
    }

    pub fn save_printed_images(&mut self) -> Result<String>
        where U: SpoolerAccess,
              ZxSpectrum<C, U>: DynSpoolerAccess
    {
        fn save_images<S: DotMatrixGfx>(info: &mut String, source: &str, spooler: &mut S) -> Result<()> {
            use fmt::Write;
            if !spooler.is_empty() {
                let mut name = format!("print_out_{}_{}", source, now_timestamp_format!());
                let base_len = name.len();
                let mut buf: Vec<u8> = Vec::new();
                if let Some((width, height)) = spooler.write_gfx_data(&mut buf) {
                    name.push_str(".png");
                    image::save_buffer(&name, &buf, width, height, image::ColorType::L8)?;
                    writeln!(info, "  {}", name)?;
                    name.truncate(base_len);
                }
                name.push_str(".svg");
                let mut file = fs::File::create(&name)?;
                spooler.write_svg_dot_gfx_lines("SDL2 Spectrusty DEMO", &mut file)?;
                writeln!(info, "  {}", name)?;
                spooler.clear();
            }
            Ok(())
        }
        let mut info = String::new();
        if let Some(spooler) = self.spectrum.ula.plus3centronics_spooler_mut() {
            save_images(&mut info, "centronics", spooler)?;
        }
        if let Some(spooler) = self.spectrum.ula.ay128rs232_spooler_mut() {
            save_images(&mut info, "128rs232", spooler)?;
        }
        if let Some(spooler) = self.spectrum.if1rs232_spooler_mut() {
            save_images(&mut info, "if1rs232", spooler)?;
        }
        if let Some(spooler) = self.spectrum.zxprinter_spooler_mut() {
            save_images(&mut info, "zxprinter", spooler)?;
        }
        if info.is_empty() {
            info.push_str("Printer spoolers are empty");
        }
        else {
            info.insert_str(0, "The following files has been created:\n\n");
        }
        Ok(info)
    }

    pub fn tape_info(&mut self) -> Result<Cow<str>> {
        use fmt::Write;
        let current = self.spectrum.state.tape.reader_ref()
                                  .map(|rd| rd.chunk_no())
                                  .unwrap_or(0);
        if let Some(mut reader) = self.spectrum.state.tape.try_reader_mut()? {
            reader.rewind();
            let iter = TapReadInfoIter::from(reader);
            let mut tapeinfo = String::with_capacity(300);
            for (info, no) in iter.zip(1..) {
                writeln!(tapeinfo, "{} {:2}: {}",
                    if no == current  { "->" } else { "  " }, no, info?)?;
            }
            Ok(tapeinfo.into())
        }
        else {
            Ok("There is no TAPe".into())
        }
    }

    pub fn handle_file<P: AsRef<Path>>(&mut self, path: P) -> Result<Option<String>>
        where U: ScreenDataProvider,
              ZxSpectrum<C, U>: ZxInterface1Access
    {
        let path = path.as_ref();
        let file = fs::File::open(path)?;
        match path.extension().and_then(OsStr::to_str) {
            Some(s) if s.eq_ignore_ascii_case("tap") => {
                let tape = &mut self.spectrum.state.tape;
                tape.stop();
                let old_tape = tape.insert_as_reader(file);
                if let Err(e) = self.tape_info() {
                    self.spectrum.state.tape.tap = old_tape;
                    return Err(e)
                }
                self.spectrum.state.tape.rewind_nth_chunk(1)?;
                Ok(Some(String::new()))
            }
            Some(s) if s.eq_ignore_ascii_case("scr") => {
                self.load_scr(file)?;
                Ok(Some(String::new()))
            }
            Some(s) if s.eq_ignore_ascii_case("mdr") => {
                self.load_mdr(file)
            }
            // Some(s) if s.eq_ignore_ascii_case("zxp") => {
            _ => {
                Ok(None)
            }
        }
    }

    pub fn load_scr<R: io::Read + io::Seek>(&mut self, scr_data: R) -> io::Result<()>
        where U: ScreenDataProvider
    {
        self.spectrum.ula.load_scr(scr_data)
    }

    pub fn load_mdr<R: io::Read>(&mut self, mdr_data: R) -> Result<Option<String>>
        where ZxSpectrum<C, U>: ZxInterface1Access
    {
        if let Some(mdrives) = self.spectrum.microdrives_mut() {
            let cartridge = MicroCartridge::from_mdr(mdr_data, 180)?;
            cartridge.validate_sectors()?;
            let cat = cartridge.catalog()?;
            if let Some(n) = (0..8).find(|n| !mdrives.is_cartridge_inserted(*n)) {
                mdrives.replace_cartridge(n, cartridge);
                return Ok(Some(if let Some(catalog) = cat {
                    format!("Inserted into drive {}:\n{}", n + 1, catalog)
                }
                else {
                    format!("Inserted into drive {}:\n(a blank cartridge)", n + 1)
                }))
            }
            else {
                return Err(io::Error::new(io::ErrorKind::Other, "No free drives").into())
            }
        }
        Err(io::Error::new(io::ErrorKind::Other, "ZX Interface 1 not installed").into())
    }

    pub fn save_screen(&self, basename: &str) -> io::Result<()>
        where U: ScreenDataProvider
    {
        let name = format!("{}.scr", basename);
        let file = fs::File::create(&name)?;
        self.spectrum.ula.save_scr(file)?;
        Ok(())
    }

    pub fn save_z80(&mut self, basename: &str) -> io::Result<String>
        where Self: SnapshotCreator,
              U: MemoryAccess + ControlUnit + Video
    {
        let name = format!("{}.z80", basename);
        let file = fs::File::create(&name)?;
        ensure_cpu_is_safe_for_snapshot(&mut self.spectrum.cpu, &mut self.spectrum.ula);
        let result = z80::save_z80v3(self, file)?;
        Ok(if !result.is_empty() {
            format!("The substantial amount of information has been lost in the selected snapshot format.\n\n {:?}",
                    result)
        }
        else {
            String::new()
        })
    }

    pub fn save_json(&self) -> io::Result<String>
        where Self: Serialize
    {
        let json_name = format!("spectrusty_{}.json", now_timestamp_format!());
        let json_file = fs::File::create(&json_name)?;
        serde_json::to_writer(json_file, self)?;
        Ok(json_name)
    }

    pub fn short_info(&mut self) -> Result<&str>
        where U: SpoolerAccess + UlaPlusMode,
              ZxSpectrum<C, U>: JoystickAccess + DynSpoolerAccess + ZxInterface1Access
    {
        use fmt::Write;
        let info = &mut self.info_text;
        info.clear();
        info.write_str(self.model.into())?;
        if self.spectrum.ula.is_ulaplus_enabled() {
            info.write_str(" 🎨")?;
        }
        if self.spectrum.state.paused {
            info.push_str(" ⏸");
        }
        else if self.spectrum.state.turbo {
            info.push_str(" 🏎️");
        }
        if let Some(name) = self.spectrum.current_joystick() {
            if name == "Sinclair" {
                write!(info, " 🕹{}{}", &name[0..1], self.spectrum.state.sub_joy + 1)?;
            }
            else {
                write!(info, " 🕹{}", &name[0..1])?;
            }
        }

        info.push_str(" ");

        self.info_range.start = info.len();
        dynamic_info(&self.spectrum, info)?;
        self.info_range.end = info.len();

        let running = self.spectrum.state.tape.running;
        // is there any TAPE inserted at all?
        if let Some(tap) = self.spectrum.state.tape.tap.as_mut() {
            let flash = if self.spectrum.state.instant_tape { "⚡⚡" }
                        else if self.spectrum.state.flash_tape { "⚡" }
                        else { " " };
            // we'll show if the TAP sound is audible
            let audible = if self.spectrum.state.audible_tape { '🔊' } else { '🔈' };
            match tap {
                Tap::Reader(..) if running => write!(info, "🖭{}{} ⏵", flash, audible)?,
                Tap::Writer(..) if running => write!(info, "🖭{}{} ⏺", flash, audible)?,
                tap => {
                    // The TAPE is paused so we'll show some TAP block metadata.
                    // This creates a TapChunkRead trait implementation that when dropped
                    // will restore underlying file seek position, so it's perfectly
                    // save to use it to read the metadata of the current chunk.
                    let mut rd = tap.try_reader_mut()?;
                    let chunk_no = rd.rewind_chunk()?;
                    let chunk_info = TapChunkInfo::try_from(rd.get_mut())?;
                    rd.done()?;
                    write!(info, "🖭{}{} {}: {}", flash, audible, chunk_no, chunk_info)?;
                }
            }
        }
        Ok(info)
    }

    pub fn update_dynamic_info(&mut self) -> Result<Option<&str>>
        where U: SpoolerAccess,
              ZxSpectrum<C, U>: DynSpoolerAccess + ZxInterface1Access
    {
        let mut dynbuf: ArrayString<[_;64]> = ArrayString::new();
        dynamic_info(&self.spectrum, &mut dynbuf)?;
        if self.info_text[self.info_range.clone()] != dynbuf[..] {
            self.info_text.replace_range(self.info_range.clone(), &dynbuf);
            self.info_range.end = self.info_range.start + dynbuf.len();
            return Ok(Some(&self.info_text))
        }
        Ok(None)
    }

    pub fn device_info(&self) -> Result<String>
        where U: SpoolerAccess + UlaPlusMode,
              ZxSpectrum<C, U>: JoystickAccess + ZxInterface1Access,
              C: fmt::Display
    {
        use fmt::Write;
        let mut info = self.model.to_string();
        write!(info, " ({} Z80)", self.spectrum.cpu)?;
        if self.spectrum.ula.is_ulaplus_enabled() {
            info.write_str(" ULAplus")?;
        }
        if self.spectrum.ula.plus3centronics_spooler_ref().is_some() {
            info.write_str("\n\t+ Centronics Printer")?;
        }
        if self.spectrum.ula.ay128rs232_spooler_ref().is_some() {
            info.write_str("\n\t+ 128k RS232 Printer")?;
        }
        if let Some(name) = self.spectrum.current_joystick() {
            if name == "Sinclair" {
                write!(info, "\n\t+ {}{} Joystick", &name, self.spectrum.state.sub_joy + 1)?;
            }
            else {
                write!(info, "\n\t+ {}  Joystick", &name)?;
            }
        }
        if let Some(devices) = self.spectrum.ula.dyn_bus_device_ref() {
            for dev in devices.into_iter() {
                write!(info, "\n\t+ {}", dev)?;
            }
        }
        if let Some(mdrives) = self.spectrum.microdrives_ref() {
            info.write_str("\nMicrodrives:\n")?;
            for n in 0..8 {
                if let Some(mdr) = mdrives.cartridge_at(n) {
                    write!(info, "{}:({}/{}) ", n + 1, mdr.count_sectors_in_use(), mdr.count_formatted())?;
                }
                else {
                    write!(info, "{}:(free) ", n + 1)?;
                }
            }
        }
        Ok(info)
    }
}

fn dynamic_info<C, U, W: fmt::Write>(spec: &ZxSpectrum<C, U>, info: &mut W) -> Result<()>
    where C: Cpu,
          U: SpoolerAccess,
          ZxSpectrum<C, U>: DynSpoolerAccess + ZxInterface1Access
{
    if let Some(microdrives) = spec.microdrives_ref() {
        if let Some((index, md)) = microdrives.cartridge_in_use() {
            write!(info, "✇{}^{:.2}", index + 1, md.head_at())?;
        }
    }

    fn print_spooling<S: DotMatrixGfx, W: fmt::Write>(info: &mut W, name: &str, spooler: &S) {
        let lines = spooler.lines_buffered();
        if lines != 0 || spooler.is_spooling() {
            write!(info, "🖶{}[{}]", name, lines).unwrap();
        }                            
    }
    if let Some(spooler) = spec.ula.plus3centronics_spooler_ref() {
        print_spooling(info, "+3P", spooler);
    }
    if let Some(spooler) = spec.ula.ay128rs232_spooler_ref() {
        print_spooling(info, "128S", spooler);
    }
    if let Some(spooler) = spec.if1rs232_spooler_ref() {
        print_spooling(info, "IF1S", spooler);
    }
    if let Some(spooler) = spec.zxprinter_spooler_ref() {
        print_spooling(info, "ZX", spooler);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use core::mem::size_of;
    use zxspectrum_common::MemTap;
    use spectrusty::memory::NoMemoryExtension;
    use spectrusty_utils::io::{Empty, Sink};
    use super::*;
    type ZxSpectrumModel<C, X, F, R, W> = zxspectrum_common::ZxSpectrumModel<C,
                                                serde::SerdeDynDevice, X, F, R, W>;

    #[test]
    fn test_sizes() {
        println!("ZxSpectrumModel no-ext  file-tap no R/W: {}", size_of::<ZxSpectrumModel<Z80Any,
                                        NoMemoryExtension,
                                        std::fs::File,
                                        Empty,
                                        Sink>>());
        println!("ZxSpectrumModel no-ext  mem-tap  no R/W: {}", size_of::<ZxSpectrumModel<Z80Any, 
                                        NoMemoryExtension,
                                        MemTap,
                                        Empty,
                                        Sink>>());
        println!("ZxSpectrumModel no-ext  file-tap io R/W: {}", size_of::<ZxSpectrumModel<Z80Any,
                                        NoMemoryExtension,
                                        std::fs::File,
                                        NonBlockingStdinReader,
                                        EpsonGfxFilteredStdoutWriter>>());
        println!("ZxSpectrumModel if1-ext file-tap io R/W: {}", size_of::<ZxSpectrumModel<Z80Any, 
                                        ZxInterface1MemExt,
                                        std::fs::File,
                                        NonBlockingStdinReader,
                                        EpsonGfxFilteredStdoutWriter>>());
    }
}
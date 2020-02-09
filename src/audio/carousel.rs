#![allow(unused_imports)]
use std::error;
use core::fmt;
use core::time::Duration;
use core::mem::{swap, replace};
use core::ops::{Deref, DerefMut};
use std::sync::mpsc::{sync_channel, SyncSender, Receiver, SendError, RecvError, TryRecvError, RecvTimeoutError};

pub use super::sample::AudioSample;

pub type AudioFrameResult<T> = Result<T, AudioFrameError>;

#[derive(Debug, Clone)]
pub struct AudioFrameError;

impl fmt::Display for AudioFrameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "the remote thread has been terminated")
    }
}

impl error::Error for AudioFrameError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl<T> From<SendError<T>> for AudioFrameError {
    fn from(_error: SendError<T>) -> Self {
        AudioFrameError
    }
}

impl From<RecvError> for AudioFrameError {
    fn from(_error: RecvError) -> Self {
        AudioFrameError
    }
}

#[derive(Clone, Debug)]
pub struct AudioBuffer<T>(pub Vec<T>);

impl<T> Deref for AudioBuffer<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AudioBuffer<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: AudioSample> AudioBuffer<T> {
    fn new(frame_samples: usize, channels: u8) -> Self {
        let size = frame_samples * channels as usize;
        AudioBuffer(vec![T::center();size])
    }
}

impl<T> AudioBuffer<T> {
    #[inline(always)]
    fn sampled_size(&self) -> usize {
        self.0.len()
    }
}

impl<T: Copy> AudioBuffer<T> {
    #[inline]
    fn copy_to(&self, target: &mut [T], src_offset: usize) -> usize {
        let end_offset = self.sampled_size().min(src_offset + target.len());
        let source = &self.0[src_offset..end_offset];
        // eprintln!("cur: {} out: {} of {} src.len: {}", cursor, target_offset, target_buffer.len(), source.len());
        let copied_size = source.len();
        target[..copied_size].copy_from_slice(source);
        copied_size
    }
}

#[derive(Debug)]
pub struct AudioFrameConsumer<T> {
    buffer: AudioBuffer<T>,
    cursor: usize,
    producer_tx: SyncSender<AudioBuffer<T>>,
    rx: Receiver<AudioBuffer<T>>,
}

#[derive(Debug)]
pub struct AudioFrameProducer<T> {
    pub buffer: AudioBuffer<T>,
    // pub sample_rate: u32,
    // pub channels: u8,
    rx: Receiver<AudioBuffer<T>>,
    consumer_tx: SyncSender<AudioBuffer<T>>,
}

pub fn create_carousel<T>(latency: usize, frame_samples: usize, channels: u8) ->
                                                (AudioFrameProducer<T>, AudioFrameConsumer<T>)
where T: 'static + AudioSample + Send
{
    // let frame_samples = (sample_rate as f64 * frame_duration).ceil() as usize;
    let buffer = AudioBuffer::<T>::new(frame_samples, channels);
    let (producer_tx, producer_rx) = sync_channel::<AudioBuffer<T>>(latency);
    let (consumer_tx, consumer_rx) = sync_channel::<AudioBuffer<T>>(latency);
    if latency > 0 {
        // Add some frame buffers into circulation
        for _ in 1..latency {
            consumer_tx.send(buffer.clone()).unwrap(); // infallible
        }
        producer_tx.send(buffer.clone()).unwrap(); // infallible
    }
    let producer = AudioFrameProducer::new(buffer.clone(), consumer_tx, producer_rx);
    let consumer = AudioFrameConsumer::new(buffer, producer_tx, consumer_rx);
    (producer, consumer)
}

impl<T> AudioFrameConsumer<T> {
    pub fn new(buffer: AudioBuffer<T>,
               producer_tx: SyncSender<AudioBuffer<T>>,
               consumer_rx: Receiver<AudioBuffer<T>>) -> Self {
        AudioFrameConsumer {
            buffer,
            cursor: 0,
            producer_tx,
            rx: consumer_rx
        }
    }

    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
    }
}

impl<T: 'static + Copy + Send> AudioFrameConsumer<T> {
    /// Receives the next frame waiting for it up to given `wait_max_ms`.
    /// On `Ok(true)` replaces the current frame with the new one and sends back the old one.
    /// If waiting for a new frame times out returns `Ok(false)`.
    /// Returns `Err(AudioFrameError)` only when sending or reveiving failed,
    /// which is possible only when the remote end has disconnected.
    #[inline]
    pub fn next_frame(&mut self, wait_max_ms: u16) -> AudioFrameResult<bool> {
        match self.rx.recv_timeout(Duration::from_millis(wait_max_ms as u64)) {
            Ok(mut buffer) => {
                swap(&mut self.buffer, &mut buffer);
                self.producer_tx.send(buffer)?;
                Ok(true)
            }
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => Err(AudioFrameError),
        }
    }

    /// Exposes current frame as a slice.
    #[inline]
    pub fn current_frame(&self) -> &[T] {
        &self.buffer
    }

    /// Fills `target_buffer` with the received audio frames repeating the process until the whole
    /// buffer is filled or optionally when the waiting for the next frame times out.
    /// On success returns the unfilled part of the target buffer in case there was a missing frame
    /// and `ignore_missing` was `false`. If the whole buffer has been filled returns an empty slice.
    /// In case `ignore_missing` is `true` the last audio frame will be rendered again.
    /// Returns `Err(AudioFrameError)` only when sending or reveiving failed,
    /// which is possible only when the remote end has disconnected.
    pub fn fill_buffer<'a>(&mut self, mut target_buffer: &'a mut[T],
                                      wait_max_ms: u16,
                                      ignore_missing: bool) -> AudioFrameResult<&'a mut[T]> {
        let mut cursor = self.cursor;
        while !target_buffer.is_empty() {
            if cursor >= self.buffer.sampled_size() {
                if !(self.next_frame(wait_max_ms)? || ignore_missing) {
                    break
                }
                cursor = 0;
            }
            let copied_size = self.buffer.copy_to(target_buffer, cursor);
            cursor += copied_size;
            target_buffer = &mut target_buffer[copied_size..];
        }
        self.cursor = cursor;
        Ok(target_buffer)
    }
}

impl<T> AudioFrameProducer<T> {
    pub fn new(buffer: AudioBuffer<T>,
               consumer_tx: SyncSender<AudioBuffer<T>>,
               producer_rx: Receiver<AudioBuffer<T>>) -> Self {
        AudioFrameProducer { buffer, rx: producer_rx, consumer_tx }
    }

    pub fn render_frame<F: FnOnce(&mut Vec<T>)>(&mut self, render: F) {
        render(&mut self.buffer);
        // eprintln!("smpl: {}", self.buffer.sampled_size);
    }
}

impl<T: 'static + Send> AudioFrameProducer<T> {
    pub fn send_frame(&mut self) -> AudioFrameResult<()> {
        // eprintln!("waiting for buffer");
        let buffer = replace(&mut self.buffer, self.rx.recv()?);
        // eprintln!("got buffer");
        self.consumer_tx.send(buffer).map_err(From::from)
        // eprintln!("sent buffer");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::f32::consts::PI;

    #[test]
    fn carousel_works() -> Result<(), Box<dyn error::Error>> {
        // eprintln!("AudioBuffer<f32>: {:?}", core::mem::size_of::<AudioBuffer<f32>>());
        // eprintln!("AudioBuffer<u16>: {:?}", core::mem::size_of::<AudioBuffer<u16>>());
        // eprintln!("SyncSender<AudioBuffer<f32>>: {:?}", core::mem::size_of::<SyncSender<AudioBuffer<f32>>>());
        // eprintln!("SyncSender<AudioBuffer<u16>>: {:?}", core::mem::size_of::<SyncSender<AudioBuffer<u16>>>());
        const TEST_SAMPLES_COUNT: usize = 20000;
        fn sinusoid(n: u16) -> f32 {
            (PI*(n as f32)/128.0).sin()
        }

        let (mut producer, mut consumer) = create_carousel::<f32>(1, 256, 1);
        let join = thread::spawn(move || {
            // thread::sleep(Duration::from_millis(250));
            let mut target = vec![0.0;800];
            let unfilled = consumer.fill_buffer(&mut target, 1, false).unwrap();
            assert_eq!(unfilled, []);
            target.resize(TEST_SAMPLES_COUNT, 0.0);
            let unfilled = consumer.fill_buffer(&mut target[800..], 1, false).unwrap();
            assert_eq!(unfilled, []);
            target
        });

        loop {
            producer.render_frame(|vec| {
                vec.clear();
                vec.extend((0..256).map(sinusoid));
            });
            if let Err(_e) = producer.send_frame() {
                break
            }
        }
        let target = join.join().unwrap();
        assert_eq!(vec![0.0;256][..], target[..256]);
        let mut template = Vec::new();
        template.extend((0..256).map(sinusoid).cycle().take(TEST_SAMPLES_COUNT-256));
        assert_eq!(TEST_SAMPLES_COUNT-256, template.len());
        assert_eq!(template[..], target[256..]);
        Ok(())
    }
}

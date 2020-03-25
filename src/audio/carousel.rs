/*! Tools for assisting audio rendering via audio frameworks that runs on separate threads.

# The Carousel

Some audio frameworks require sample generators to be run in a loop on a separate thread 
while sound is playing or via callbacks just in time when audio buffer need to be refilled.

When emulating Spectrum computer we have to synchronize frames with video as well as audio
and having an independent thread for rendering audio frames makes this task somewhat difficult.

To ease this task the "Carousel" was implemented. Basically it consists of an [audio producer] and
an [audio consumer]. The audio producer lives in the same thread where the emulation is run
and where sound is being produced. The audio consumer is delegated to the audio thread and its
role is to relay audio samples to the audio framework.

```text
                                 (new sample data)
                    /----> AudioBuffer ----> AudioBuffer ---->\
+----------------------+                                  +----------------------+
|  AudioFrameProducer  |                                  |  AudioFrameConsumer  | -> 🔊
+----------------------+                                  +----------------------+
                    \<---- AudioBuffer <---- AudioBuffer -----/
                                 (recycled buffers)
```
The produced [audio buffer]s ready to be played are being sent via [mpsc::channel] from
the [audio producer] to the [audio consumer]. The consumer fills the audio buffers provided by
the audio framework with samples from the received [audio buffer] frames and sends the used
frame buffers back via another channel to the [audio producer] to be filled again with new
frame data.

Each [audio buffer] size is determined by the emulated frame duration and is independent from
the audio framework output buffer size.

The number of buffers in circulation determines the audio latency.

[audio producer]: AudioFrameProducer
[audio consumer]: AudioFrameConsumer
[audio buffer]: AudioBuffer
[mpsc::channel]: std::sync::mpsc::channel
*/
use std::error;
use core::fmt;

use core::mem::{swap, replace};
use core::ops::{Deref, DerefMut};
use std::sync::mpsc::{channel, Sender, Receiver, SendError, RecvError,
                        TryRecvError, RecvTimeoutError, TrySendError};

pub use super::sample::AudioSample;

pub type AudioFrameResult<T> = Result<T, AudioFrameError>;

#[derive(Debug, Clone)]
pub struct AudioFrameError;

/// The audio buffer is a carrier of audio samples generated for every emulated frame.
///
/// The format and number of channels depends on the audio framework requirements.
#[derive(Clone, Debug)]
pub struct AudioBuffer<T>(pub Vec<T>);

/// Relays [AudioBuffer] samples to the audio framework output buffers.
#[derive(Debug)]
pub struct AudioFrameConsumer<T> {
    buffer: AudioBuffer<T>,
    cursor: usize,
    producer_tx: Sender<AudioBuffer<T>>,
    rx: Receiver<AudioBuffer<T>>,
}

/// Allows to relay rendered [AudioBuffer] to the [AudioFrameConsumer].
#[derive(Debug)]
pub struct AudioFrameProducer<T> {
    /// The next audio buffer frame to render samples to.
    pub buffer: AudioBuffer<T>,
    rx: Receiver<AudioBuffer<T>>,
    consumer_tx: Sender<AudioBuffer<T>>,
}

/// Creates an inter-connected pair or [AudioFrameProducer] and [AudioFrameConsumer].
///
/// The `latency` + 1 specifies how many buffers will be circulating in the carousel.
/// The good indicator of how many is needed depends on the size of the target audio
/// buffers provided by the framework. The size of the target audio buffer / size of
/// the produced frame buffers is a good approximation.
///
/// Basically the larger the `latency` is the more stable the output sound stream will
/// be, but at the cost of more delayed playback. Implementations should set a good
/// default based on experiments but may allow users to adjust this value eventually.
///
/// `sample_frames` and `channels` determines the size of the allocated buffers.
pub fn create_carousel<T>(latency: usize, sample_frames: usize, channels: u8) ->
                                                (AudioFrameProducer<T>, AudioFrameConsumer<T>)
where T: 'static + AudioSample + Send
{
    // let sample_frames = (sample_rate as f64 * frame_duration).ceil() as usize;
    let buffer = AudioBuffer::<T>::new(sample_frames, channels);
    let (producer_tx, producer_rx) = channel::<AudioBuffer<T>>();
    let (consumer_tx, consumer_rx) = channel::<AudioBuffer<T>>();
    // if latency > 0 {
        // Add some frame buffers into circulation
        // for _ in 0..latency {
            producer_tx.send(buffer.clone()).unwrap(); // infallible
        // }
        for _ in 0..latency {
            consumer_tx.send(buffer.clone()).unwrap(); // infallible
        }
        // }
    // }
    let producer = AudioFrameProducer::new(buffer.clone(), consumer_tx, producer_rx);
    let consumer = AudioFrameConsumer::new(buffer, producer_tx, consumer_rx);
    (producer, consumer)
}

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

impl<T> From<TrySendError<T>> for AudioFrameError {
    fn from(_error: TrySendError<T>) -> Self {
        AudioFrameError
    }
}

impl<T> From<SendError<T>> for AudioFrameError {
    fn from(_error: SendError<T>) -> Self {
        AudioFrameError
    }
}

impl From<TryRecvError> for AudioFrameError {
    fn from(_error: TryRecvError) -> Self {
        AudioFrameError
    }
}

impl From<RecvError> for AudioFrameError {
    fn from(_error: RecvError) -> Self {
        AudioFrameError
    }
}

impl From<RecvTimeoutError> for AudioFrameError {
    fn from(_error: RecvTimeoutError) -> Self {
        AudioFrameError
    }
}

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
    fn new(sample_frames: usize, channels: u8) -> Self {
        let size = sample_frames * channels as usize;
        AudioBuffer(vec![T::silence();size])
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

impl<T> AudioFrameConsumer<T> {
    /// Creates a new instance of `AudioFrameConsumer`.
    ///
    /// Prefer to use [create_carousel] instead.
    pub fn new(buffer: AudioBuffer<T>,
               producer_tx: Sender<AudioBuffer<T>>,
               consumer_rx: Receiver<AudioBuffer<T>>) -> Self {
        AudioFrameConsumer {
            buffer,
            cursor: 0,
            producer_tx,
            rx: consumer_rx
        }
    }
    /// Resets the audio buffer sample cursor.
    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
    }
}

impl<T: 'static + Copy + Send> AudioFrameConsumer<T> {
    /// Attempts to receive the next audio frame from the [AudioFrameProducer].
    ///
    /// When `Ok(true)` is returned replaces the current frame buffer with the one received
    /// and sends back the current one.
    ///
    /// If there is no new buffer waiting in the message queue returns `Ok(false)`.
    ///
    /// Returns `Err(AudioFrameError)` only when sending or reveiving has failed,
    /// which is possible only when the remote end has disconnected.
    #[inline]
    pub fn next_frame(&mut self) -> AudioFrameResult<bool> {
        match self.rx.try_recv() {
        // match self.rx.recv_timeout(Duration::from_millis(wait_max_ms as u64)) {
            Ok(mut buffer) => {
                // print!("{:?} ", buffer.as_ptr());
                swap(&mut self.buffer, &mut buffer);
                self.producer_tx.send(buffer)?;
                // let mut buffer = Some(buffer);
                // loop {
                //     match self.producer_tx.send(buffer.take().unwrap()) {
                //         Err(TrySendError::Full(buf)) => {
                //             println!("cons couldn't send");
                //             buffer = Some(buf)
                //         }
                //         Ok(()) => break,
                //         Err(e) => Err(e)?,
                //     };
                // }
                Ok(true)
            }
            Err(TryRecvError::Empty) => {
                Ok(false)
            },
            Err(TryRecvError::Disconnected) => Err(AudioFrameError)
            // Err(RecvTimeoutError::Timeout) => Ok(false),
            // Err(RecvTimeoutError::Disconnected) => Err(AudioFrameError),
        }
    }
    /// Exposes last received frame buffer as a slice of samples.
    #[inline]
    pub fn current_frame(&self) -> &[T] {
        &self.buffer
    }
    /// Fills the `target_buffer` with the received audio frame samples.
    ///
    /// Attempts to receive new frame buffers when necessary, repeating the process until 
    /// the whole buffer is filled or when there are no more buffers waiting in the incoming
    /// queue.
    ///
    /// Returns the unfilled part of the target buffer in case there was no more frames to receive
    /// and `ignore_missing` was `false`.
    ///
    /// If the whole buffer has been filled returns an empty slice.
    ///
    /// In case `ignore_missing` is `true` the last audio frame will be rendered again if there are
    /// no more new buffers in the queue.
    ///
    /// Returns `Err(AudioFrameError)` only when sending or receiving buffers has failed,
    /// which is possible only when the remote end has disconnected.
    pub fn fill_buffer<'a>(
                &mut self,
                mut target_buffer: &'a mut[T],
                ignore_missing: bool
            ) -> AudioFrameResult<&'a mut[T]>
    {
        let mut cursor = self.cursor;
        while !target_buffer.is_empty() {
            if cursor >= self.buffer.sampled_size() {
                if !(self.next_frame()? || ignore_missing) {
                    break
                }
                cursor = 0;
            }
            // print!("{:?} ", self.buffer.as_ptr());
            let copied_size = self.buffer.copy_to(target_buffer, cursor);
            cursor += copied_size;
            target_buffer = &mut target_buffer[copied_size..];
        }
        self.cursor = cursor;
        Ok(target_buffer)
    }
}

impl<T> AudioFrameProducer<T> {
    /// Creates a new instance of `AudioFrameProducer`.
    ///
    /// Prefer to use [create_carousel] instead.
    pub fn new(buffer: AudioBuffer<T>,
               consumer_tx: Sender<AudioBuffer<T>>,
               producer_rx: Receiver<AudioBuffer<T>>) -> Self {
        AudioFrameProducer { buffer, rx: producer_rx, consumer_tx }
    }
    /// Provides the current frame buffer as `Vec` of samples for rendering via a closure.
    ///
    /// The closure should ensure the size of the `Vec` is resized to the number of actually
    /// rendered samples.
    pub fn render_frame<F: FnOnce(&mut Vec<T>)>(&mut self, render: F) {
        render(&mut self.buffer);
        // eprintln!("smpl: {}", self.buffer.sampled_size);
    }
}

impl<T: 'static + Send> AudioFrameProducer<T> {
    /// Sends the audio frame buffer to the [AudioFrameConsumer] and replaces it with a recycled
    /// buffer received back from [AudioFrameConsumer].
    ///
    /// This method will block if the recycled buffer queue is empty.
    ///
    /// Returns `Err(AudioFrameError)` only when sending or receiving buffers has failed,
    /// which is possible only when the remote end has disconnected.
    pub fn send_frame(&mut self) -> AudioFrameResult<()> {
        // eprintln!("waiting for buffer");
        // let buffer = loop {
        //     match self.rx.try_recv() {
        //         Ok(buf) => break buf,
        //         Err(TryRecvError::Empty) => {
        //             let now = std::time::Instant::now();
        //             let buf = self.rx.recv()?;
        //             println!("prod couldn't recv, {:?}", now.elapsed());
        //             break buf;
        //         }
        //         Err(e) => Err(e)?
        //     }
        // };
        // let mut buffer = Some(replace(&mut self.buffer, buffer));
        // let buffer = replace(&mut self.buffer, buffer);
        // eprintln!("got buffer");
        // loop {
        //     match self.consumer_tx.try_send(buffer.take().unwrap()) {
        //         Err(TrySendError::Full(buf)) => {
        //             println!("prod couldn't send");
        //             buffer = Some(buf)
        //         }
        //         Ok(()) => return Ok(()),
        //         Err(e) => Err(e)?
        //     }
        // }
        let buffer = replace(&mut self.buffer, self.rx.recv()?);
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
        // eprintln!("Sender<AudioBuffer<f32>>: {:?}", core::mem::size_of::<Sender<AudioBuffer<f32>>>());
        // eprintln!("Sender<AudioBuffer<u16>>: {:?}", core::mem::size_of::<Sender<AudioBuffer<u16>>>());
        const TEST_SAMPLES_COUNT: usize = 20000;
        const LATENCY: usize = 5;
        const BUFSIZE: usize = 256;
        const ZEROLEN: usize = BUFSIZE + LATENCY*BUFSIZE;
        fn sinusoid(n: u16) -> f32 {
            (PI*(n as f32)/BUFSIZE as f32).sin()
        }

        let (mut producer, mut consumer) = create_carousel::<f32>(LATENCY, BUFSIZE, 1);
        let join = thread::spawn(move || {
            let mut target = vec![0.0;800];
            let mut unfilled = &mut target[..];
            loop {
                thread::sleep(std::time::Duration::from_millis(1));
                unfilled = consumer.fill_buffer(unfilled, false).unwrap();
                if unfilled.len() == 0 {
                    break;
                }
            }
            target.resize(TEST_SAMPLES_COUNT, 0.0);
            let mut unfilled = &mut target[800..];
            loop {
                thread::sleep(std::time::Duration::from_millis(1));
                unfilled = consumer.fill_buffer(unfilled, false).unwrap();
                if unfilled.len() == 0 {
                    break;
                }
            }
            target
        });

        loop {
            producer.render_frame(|vec| {
                vec.clear();
                vec.extend((0..BUFSIZE as u16).map(sinusoid));
            });
            if let Err(_e) = producer.send_frame() {
                break
            }
        }
        let target = join.join().unwrap();
        assert_eq!(vec![0.0;ZEROLEN][..], target[..ZEROLEN]);
        let mut template = Vec::new();
        template.extend((0..BUFSIZE as u16).map(sinusoid).cycle().take(TEST_SAMPLES_COUNT-ZEROLEN));
        assert_eq!(TEST_SAMPLES_COUNT-ZEROLEN, template.len());
        assert_eq!(template[..], target[ZEROLEN..]);
        Ok(())
    }
}

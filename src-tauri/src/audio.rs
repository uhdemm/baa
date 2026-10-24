//! Microphone capture. A dedicated thread owns the cpal stream (streams aren't
//! Send on every platform); the rest of the app talks to it over a channel.
//! The stream is only open while recording, so the mic light means "listening".

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TARGET_RATE: u32 = 16_000;

pub type LevelFn = Arc<dyn Fn(f32) + Send + Sync>;

enum Cmd {
    Start { device: Option<String>, reply: Sender<anyhow::Result<()>> },
    Stop { reply: Sender<Vec<f32>> },
    Cancel,
}

pub struct Recorder {
    tx: Sender<Cmd>,
}

impl Recorder {
    pub fn new(on_level: LevelFn) -> Self {
        let (tx, rx) = channel::<Cmd>();
        std::thread::Builder::new()
            .name("baa-audio".into())
            .spawn(move || {
                let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
                let mut stream: Option<(cpal::Stream, u32)> = None;
                for cmd in rx {
                    match cmd {
                        Cmd::Start { device, reply } => {
                            stream = None;
                            buf.lock().clear();
                            let r = open_stream(device.as_deref(), buf.clone(), on_level.clone());
                            match r {
                                Ok(s) => {
                                    stream = Some(s);
                                    let _ = reply.send(Ok(()));
                                }
                                Err(e) => {
                                    let _ = reply.send(Err(e));
                                }
                            }
                        }
                        Cmd::Stop { reply } => {
                            let rate = stream.take().map(|(s, r)| {
                                drop(s);
                                r
                            });
                            let samples = std::mem::take(&mut *buf.lock());
                            let out = match rate {
                                Some(r) => resample(&samples, r),
                                None => vec![],
                            };
                            let _ = reply.send(out);
                        }
                        Cmd::Cancel => {
                            stream = None;
                            buf.lock().clear();
                        }
                    }
                }
            })
            .expect("audio thread");
        Recorder { tx }
    }

    pub fn start(&self, device: Option<String>) -> anyhow::Result<()> {
        let (reply, rx) = channel();
        self.tx.send(Cmd::Start { device, reply })?;
        rx.recv_timeout(Duration::from_secs(5))?
    }

    /// Stops recording and returns 16 kHz mono samples.
    pub fn stop(&self) -> Vec<f32> {
        let (reply, rx) = channel();
        if self.tx.send(Cmd::Stop { reply }).is_err() {
            return vec![];
        }
        rx.recv_timeout(Duration::from_secs(10)).unwrap_or_default()
    }

    pub fn cancel(&self) {
        let _ = self.tx.send(Cmd::Cancel);
    }
}

pub fn list_inputs() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn pick_device(name: Option<&str>) -> anyhow::Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(want) = name {
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                if d.name().ok().as_deref() == Some(want) {
                    return Ok(d);
                }
            }
        }
    }
    host.default_input_device().ok_or_else(|| anyhow::anyhow!("No microphone found"))
}

fn open_stream(
    device: Option<&str>,
    buf: Arc<Mutex<Vec<f32>>>,
    on_level: LevelFn,
) -> anyhow::Result<(cpal::Stream, u32)> {
    let dev = pick_device(device)?;
    let cfg = dev.default_input_config()?;
    let rate = cfg.sample_rate().0;
    let channels = cfg.channels() as usize;
    let format = cfg.sample_format();
    let config: cpal::StreamConfig = cfg.into();

    // Level meter: RMS over each callback, sent ~30 times a second.
    let last = Arc::new(Mutex::new(Instant::now()));
    let push = move |mono: &[f32]| {
        let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
        buf.lock().extend_from_slice(mono);
        let mut l = last.lock();
        if l.elapsed() >= Duration::from_millis(33) {
            *l = Instant::now();
            // speech RMS sits around 0.02-0.2; map to a 0..1 meter
            on_level((rms * 6.0).sqrt().min(1.0));
        }
    };
    let err = |e| eprintln!("baa audio error: {e}");

    macro_rules! build {
        ($t:ty, $conv:expr) => {{
            let push = push.clone();
            dev.build_input_stream(
                &config,
                move |data: &[$t], _| {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|f| f.iter().map(|&s| $conv(s)).sum::<f32>() / channels as f32)
                        .collect();
                    push(&mono);
                },
                err,
                None,
            )?
        }};
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => build!(i16, |s: i16| s as f32 / i16::MAX as f32),
        cpal::SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
        cpal::SampleFormat::I32 => build!(i32, |s: i32| s as f32 / i32::MAX as f32),
        f => anyhow::bail!("Unsupported mic sample format {f:?}"),
    };
    stream.play()?;
    Ok((stream, rate))
}

fn resample(input: &[f32], from: u32) -> Vec<f32> {
    use rubato::{FftFixedIn, Resampler};
    if from == TARGET_RATE || input.is_empty() {
        return input.to_vec();
    }
    let chunk = 1024;
    let Ok(mut r) = FftFixedIn::<f32>::new(from as usize, TARGET_RATE as usize, chunk, 2, 1) else {
        return input.to_vec();
    };
    let mut out = Vec::with_capacity(input.len() * TARGET_RATE as usize / from as usize + chunk);
    let mut pos = 0;
    while pos + r.input_frames_next() <= input.len() {
        let n = r.input_frames_next();
        if let Ok(v) = r.process(&[&input[pos..pos + n]], None) {
            out.extend_from_slice(&v[0]);
        }
        pos += n;
    }
    if pos < input.len() {
        if let Ok(v) = r.process_partial(Some(&[&input[pos..]]), None) {
            out.extend_from_slice(&v[0]);
        }
    }
    // flush the resampler's internal delay so the last word isn't clipped
    if let Ok(v) = r.process_partial::<&[f32]>(None, None) {
        out.extend_from_slice(&v[0]);
    }
    out
}

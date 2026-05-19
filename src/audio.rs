// src/audio.rs
//
// Host-audio frontend. Owns a cpal output stream and a shared ring
// buffer between the emulator thread and the audio thread.
//
// Architecture:
//   - The emulator thread produces interleaved stereo f32 samples from
//     the APU and pushes them into the ring buffer via try_push.
//   - cpal's callback thread (managed by the OS, real-time priority)
//     calls the closure passed to build_output_stream, which pops
//     samples out of the ring.
//   - When the ring fills, try_push returns "not all consumed" and the
//     emulator-side caller can sleep briefly and retry. That is what
//     turns audio into the timing source for the whole emulator —
//     when audio plays at real-time speed, the emulator does too.
//
// We use a Mutex<VecDeque<f32>> rather than a lockfree SPSC crate to
// avoid an extra dependency. At 48 kHz × 2 channels the data rate is
// trivial and lock hold times are sub-microsecond; this is also the
// pattern cpal's own examples use.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

/// Target audio latency. The ring buffer is sized to hold this many
/// milliseconds of stereo samples; anything beyond that gets back-
/// pressured onto the emulator thread.
const TARGET_LATENCY_MS: u32 = 100;

/// The shared queue. Wrapped in Arc<Mutex<…>> so both threads can hold a
/// clone.
type SharedRing = Arc<Mutex<VecDeque<f32>>>;

/// Owns the cpal stream + ring buffer. Drop this to stop audio.
pub struct AudioBackend {
    pub sample_rate: u32,

    ring: SharedRing,
    /// Maximum number of f32 entries the ring will hold. Pushes beyond
    /// this point either block (via the caller) or drop samples.
    capacity: usize,

    /// Keep the stream alive. cpal stops a stream when the handle drops,
    /// so we have to hold onto it for the program's lifetime.
    _stream: cpal::Stream,
}

impl AudioBackend {
    /// Open the default output device, build an output stream, and
    /// return the live backend. The returned `sample_rate` is whatever
    /// the device chose — typically 44100 or 48000 — and the APU
    /// should be configured to emit at that rate.
    pub fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default audio output device available".to_string())?;

        let config = device
            .default_output_config()
            .map_err(|e| format!("default output config: {}", e))?;

        let sample_format = config.sample_format();
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        if channels < 2 {
            return Err(format!(
                "device reports {} channels; need at least 2 for stereo",
                channels
            ));
        }

        // Ring buffer sized to TARGET_LATENCY_MS of stereo audio.
        let capacity =
            (sample_rate as usize * 2 * TARGET_LATENCY_MS as usize) / 1000;
        let ring: SharedRing = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));

        // Build the cpal stream. The callback's job is to fill `data`
        // from the ring; if the ring is empty (underrun) it writes
        // silence so the audio hardware doesn't loop the previous
        // block.
        let stream_config: StreamConfig = config.clone().into();
        let ring_for_cb = Arc::clone(&ring);

        let stream = match sample_format {
            SampleFormat::F32 => build_stream_f32(&device, &stream_config, ring_for_cb, channels)?,
            SampleFormat::I16 => build_stream_i16(&device, &stream_config, ring_for_cb, channels)?,
            SampleFormat::U16 => build_stream_u16(&device, &stream_config, ring_for_cb, channels)?,
            other => return Err(format!("unsupported sample format: {:?}", other)),
        };

        stream.play().map_err(|e| format!("stream.play(): {}", e))?;

        Ok(Self {
            sample_rate,
            ring,
            capacity,
            _stream: stream,
        })
    }

    /// Push as many of `samples` as currently fit into the ring; return
    /// the number actually pushed. Caller is expected to retry the
    /// remainder (after a sleep) or drop it.
    pub fn try_push(&self, samples: &[f32]) -> usize {
        let mut ring = self.ring.lock().expect("audio ring mutex poisoned");
        let space = self.capacity.saturating_sub(ring.len());
        let n = samples.len().min(space);
        ring.extend(&samples[..n]);
        n
    }

    /// True if the ring is at or above the latency target — i.e. there
    /// is no reason to push more samples right now. Used by the run
    /// loop to decide when to sleep.
    pub fn is_full(&self) -> bool {
        let ring = self.ring.lock().expect("audio ring mutex poisoned");
        ring.len() >= self.capacity
    }
}

// ── stream builders for each sample format ─────────────────────────────
//
// cpal can hand us back any sample format the device supports. We
// always *think* in f32 (range -1..1); the format-specific closure
// converts on output.

fn build_stream_f32(
    device: &cpal::Device,
    config: &StreamConfig,
    ring: SharedRing,
    channels: usize,
) -> Result<cpal::Stream, String> {
    let err_fn = |err| eprintln!("audio stream error: {}", err);
    device
        .build_output_stream(
            config,
            move |data: &mut [f32], _| {
                let mut ring = ring.lock().expect("audio ring mutex poisoned");
                for frame in data.chunks_mut(channels) {
                    let (l, r) = pop_pair(&mut ring);
                    frame[0] = l;
                    frame[1] = r;
                    // If the device has > 2 channels, fan stereo to the
                    // rest as a sensible default (e.g. surround setups).
                    for slot in &mut frame[2..] {
                        *slot = 0.0;
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("build_output_stream f32: {}", e))
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &StreamConfig,
    ring: SharedRing,
    channels: usize,
) -> Result<cpal::Stream, String> {
    let err_fn = |err| eprintln!("audio stream error: {}", err);
    device
        .build_output_stream(
            config,
            move |data: &mut [i16], _| {
                let mut ring = ring.lock().expect("audio ring mutex poisoned");
                for frame in data.chunks_mut(channels) {
                    let (l, r) = pop_pair(&mut ring);
                    frame[0] = f32_to_i16(l);
                    frame[1] = f32_to_i16(r);
                    for slot in &mut frame[2..] {
                        *slot = 0;
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("build_output_stream i16: {}", e))
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &StreamConfig,
    ring: SharedRing,
    channels: usize,
) -> Result<cpal::Stream, String> {
    let err_fn = |err| eprintln!("audio stream error: {}", err);
    device
        .build_output_stream(
            config,
            move |data: &mut [u16], _| {
                let mut ring = ring.lock().expect("audio ring mutex poisoned");
                for frame in data.chunks_mut(channels) {
                    let (l, r) = pop_pair(&mut ring);
                    frame[0] = f32_to_u16(l);
                    frame[1] = f32_to_u16(r);
                    for slot in &mut frame[2..] {
                        *slot = 0x8000;
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("build_output_stream u16: {}", e))
}

/// Pop one stereo pair from the ring, returning (0.0, 0.0) on underrun.
/// Underruns can happen on startup (before the emulator has produced
/// any samples) and during heavy CPU load on the host.
fn pop_pair(ring: &mut VecDeque<f32>) -> (f32, f32) {
    let l = ring.pop_front().unwrap_or(0.0);
    let r = ring.pop_front().unwrap_or(0.0);
    (l, r)
}

fn f32_to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn f32_to_u16(s: f32) -> u16 {
    let signed = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i32;
    (signed + 0x8000) as u16
}
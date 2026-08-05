//! The cpal input stream, and the one queue that crosses out of the audio callback.
//!
//! The callback does the minimum spec §3.3 permits and nothing else: convert f32 → f64 at the
//! block boundary, take channel 0, run the weighting chain, sum `p²` and `n`, and push one
//! [`BlockSummary`] per block into a bounded lock-free SPSC queue. No locks, no allocation, no
//! logging, no `Mutex`. Everything downstream of that queue runs on the display side.
//!
//! The queue is bounded at ~1 s of blocks on purpose (spec §6.10): if the draining side
//! stalls, pushes fail and the blocks are simply never accounted for, so the failure shows up
//! as **lost coverage** — a number spec §6.4 publishes unconditionally — rather than as a
//! plausible, wrong level. Overflow degrades into a smaller honest sample, never a wrong
//! answer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, RingBuffer};
use serde::Serialize;

use crate::session;

/// One audio block, reduced to the only three things the metrics ring needs.
///
/// Sum-of-squares **and** count, never a mean: spec §6.1 divides by the actual sample total
/// across the whole window, so a per-block mean would have to be un-averaged to be usable.
#[derive(Debug, Clone, Copy)]
pub struct BlockSummary {
    /// Σ p² over the block, after weighting, in f64.
    pub sum_sq: f64,
    /// Samples that went into `sum_sq` — frames, not interleaved samples, since only
    /// channel 0 is read.
    pub n: u32,
    /// When the callback ran, on the monotonic clock. The ring is advanced by the clock at
    /// drain time (spec §6.2), so this is not what places the block; it is what tells the
    /// display side how long it has been since audio last arrived (spec §6.9's 200 ms).
    pub t: Instant,
}

/// What the stream and the session actually gave us, as opposed to what was asked for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureFacts {
    pub device: String,
    pub sample_format: String,
    /// Ground truth for every weighting coefficient and FFT bin downstream.
    pub sample_rate: u32,
    pub channels: u16,
    /// What the stream reports once running, which the CoreAudio backends recompute from the
    /// live device rather than echoing our request back.
    pub buffer_frames: Option<u32>,
    /// `None` off iOS, where there is no session to configure.
    pub session: Option<session::SessionFacts>,
}

/// Capture is either still coming up, running, or dead with a reason. There is no fourth
/// state at this tier — restarting a dead stream is `b07`'s job.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum CaptureState {
    Starting,
    Running(CaptureFacts),
    Failed { reason: String },
}

/// The capture side, owned by the app and shared with the audio callback.
pub struct Capture {
    state: Mutex<CaptureState>,
    /// `None` until the stream is built. Locked only by the draining side; the callback never
    /// touches it.
    consumer: Mutex<Option<Consumer<BlockSummary>>>,
    /// Blocks the queue had no room for. Bumped from the callback, which is why it is an
    /// atomic and not a counter behind the same lock.
    dropped: Arc<AtomicU64>,
    /// Errors delivered to cpal's error callback, which runs on a notification queue rather
    /// than the audio thread — so locking here is fine.
    last_error: Arc<Mutex<Option<String>>>,
}

impl Capture {
    /// Starts capture on a dedicated thread and returns immediately.
    ///
    /// Non-blocking on purpose. [`session::configure`] waits on a permission prompt whose
    /// completion block is delivered on the main queue, so doing any of this inline in Tauri's
    /// `setup` — which runs on the main thread — would deadlock the app before it drew a
    /// frame.
    pub fn start() -> Arc<Capture> {
        let capture = Arc::new(Capture {
            state: Mutex::new(CaptureState::Starting),
            consumer: Mutex::new(None),
            dropped: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(Mutex::new(None)),
        });

        let owned = Arc::clone(&capture);
        std::thread::Builder::new()
            .name("capture".into())
            .spawn(move || {
                match build(&owned) {
                    Ok(stream) => {
                        // `cpal::Stream` is `!Send` and stops on drop, so it has to stay on
                        // the thread that built it, alive for as long as the app is. Parking
                        // is where `b07`'s supervisor loop goes.
                        let _stream = stream;
                        loop {
                            std::thread::park();
                        }
                    }
                    Err(reason) => {
                        eprintln!("capture: {reason}");
                        *owned.state.lock().unwrap() = CaptureState::Failed { reason };
                    }
                }
            })
            .expect("spawn capture thread");

        capture
    }

    /// Takes every block summary waiting in the queue, oldest first, and returns how many.
    ///
    /// This is the seam the metrics pipeline drains at 10 Hz (`b03`, `b05`).
    pub fn drain<F: FnMut(BlockSummary)>(&self, mut f: F) -> usize {
        let mut guard = self.consumer.lock().unwrap();
        let Some(consumer) = guard.as_mut() else {
            return 0;
        };
        let mut drained = 0;
        while let Ok(summary) = consumer.pop() {
            f(summary);
            drained += 1;
        }
        drained
    }

    pub fn state(&self) -> CaptureState {
        self.state.lock().unwrap().clone()
    }

    /// Blocks lost to queue overflow since startup. Lost coverage, by construction.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// The most recent stream error, if any. Detail lives in the log; this exists so the
    /// device readout is not silent about a stream that died (spec §9.4's `unavailable`
    /// conflates causes on purpose).
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().clone()
    }
}

/// Configures the session, builds the stream, and publishes the facts. Runs on the capture
/// thread; never on the main thread.
fn build(capture: &Arc<Capture>) -> Result<cpal::Stream, String> {
    // Before cpal is touched. Apple's default `SoloAmbient` category permits no input, and
    // cpal's iOS backend reads the live session at call time — so a stream built first would
    // be built against the wrong rate even if it built at all.
    let session = session::configure()?;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let device_name = device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| "<unnamed>".to_string());

    let supported = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;
    let sample_format = supported.sample_format();
    if sample_format != cpal::SampleFormat::F32 {
        // Both CoreAudio backends hand us f32, which is what spec §3.3 is written against.
        // Anything else would need a conversion the callback should not be doing, so it is a
        // refusal rather than a silent widening.
        return Err(format!(
            "input device reports {sample_format} samples; only f32 is supported"
        ));
    }
    let mut config: cpal::StreamConfig = supported.into();

    // Cross-check cpal against the session. cpal's iOS backend reports the session's current
    // rate as a single-point range, so these must agree — if they do not, one of the two is
    // reading stale state and every weighting coefficient downstream is derived from the
    // wrong number.
    if let Some(facts) = &session {
        if facts.sample_rate as u32 != config.sample_rate {
            eprintln!(
                "capture: rate disagreement — AVAudioSession says {} Hz, cpal says {} Hz",
                facts.sample_rate, config.sample_rate
            );
        }
    }

    config.buffer_size = cpal::BufferSize::Fixed(session::PREFERRED_BUFFER_FRAMES);

    // ~1 s of blocks (spec §6.10). Deliberately not generous: a queue big enough to ride out a
    // long stall would let stale blocks land in the wrong 100 ms slot, which is a wrong number
    // where a dropped block is only a smaller one.
    let capacity = ((config.sample_rate as f64 / session::PREFERRED_BUFFER_FRAMES as f64).ceil()
        as usize)
        .clamp(16, 512);
    let (mut producer, consumer) = RingBuffer::<BlockSummary>::new(capacity);

    // Interleaved frame stride. Channel 0 only when the stream reports more than one channel
    // (spec §3.3, §16.9): averaging correlated channels moves the level by up to 6 dB and
    // would make the calibration offset depend on the channel count. `chunks_exact(1)` is the
    // mono case, so there is one loop and one place the weighting chain plugs in.
    let stride = config.channels.max(1) as usize;
    let dropped = Arc::clone(&capture.dropped);

    let last_error = Arc::clone(&capture.last_error);
    let error_callback = move |err: cpal::Error| {
        // Not the audio thread: cpal delivers this on a notification queue.
        eprintln!("capture: stream error {:?}: {err}", err.kind());
        *last_error.lock().unwrap() = Some(format!("{:?}: {err}", err.kind()));
    };

    let stream = device
        .build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let t = Instant::now();
                let mut sum_sq = 0.0f64;
                let mut n: u32 = 0;

                for frame in data.chunks_exact(stride) {
                    // f32 → f64 at the block boundary, and all DSP in f64 from here on: `03`
                    // measured f32 filter state degrading the level by +1.7 dB with a DC
                    // offset and +3.9 dB with 5 Hz rumble, which is exactly what a phone
                    // microphone in a venue delivers.
                    let x = frame[0] as f64;

                    // ─── the weighting seam ────────────────────────────────────────────
                    // `b03` replaces this line with the selected chain from `b02`:
                    //     let y = chain.process(x);
                    // Until then the sum is unweighted, which is Z — a real mode, not a stub.
                    // No DC blocker here or there: the A and C filters are themselves
                    // high-passes, f64 removes the precision motive, and the FFT tap wants
                    // the DC (spec §3.3).
                    let y = x;
                    // ───────────────────────────────────────────────────────────────────

                    sum_sq += y * y;
                    n += 1;
                }

                if producer.push(BlockSummary { sum_sq, n, t }).is_err() {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            error_callback,
            None,
        )
        .map_err(|e| format!("build_input_stream: {e}"))?;
    stream.play().map_err(|e| format!("play: {e}"))?;

    let facts = CaptureFacts {
        device: device_name,
        sample_format: sample_format.to_string(),
        sample_rate: config.sample_rate,
        channels: config.channels,
        buffer_frames: stream.buffer_size().ok(),
        session,
    };
    eprintln!("capture: running {facts:?}");

    *capture.consumer.lock().unwrap() = Some(consumer);
    *capture.state.lock().unwrap() = CaptureState::Running(facts);

    Ok(stream)
}

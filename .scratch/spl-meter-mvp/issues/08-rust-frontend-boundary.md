# Rust ↔ frontend boundary: what crosses, how often, in what shape

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open — unblocked, on the frontier
Blocked by: —  (was 05, now resolved)

**Inherited from ticket [`05`](05-level-metrics-pipeline.md) — the meter half of this boundary is
now fully specified, so only the spectrogram is left to design.**

Per 10 Hz display tick, the meter payload is exactly:

- **three `Option<f64>`** — rolling L_eq, instantaneous level, max hold. `None` means the display
  shows `--`, and it is a real state rather than an error: the instantaneous readout publishes
  `None` after 200 ms with no audio block, and the L_eq publishes `None` at zero coverage where it
  is arithmetically undefined (`05` decision 11).
- **coverage in seconds**, and the window length in seconds — `33s of 60s`. Published **always**,
  not only when degraded; a figure that appears only when something is wrong is a warning, which
  charting ruled out.

Two constraints on the design:

- **Every dB value crossing the bridge is already calibrated** (`05` decision 13 applies the offset
  post-log in Rust). Deliberate, so that an uncalibrated number cannot be rendered by mistake. Do
  not move the offset to the frontend for convenience.
- **All four settings are Rust-owned** — weighting (C/A/Z), time weighting (F/S), window length, and
  the calibration offset. The frontend issues commands; it holds no authoritative state. `04` had
  already implied this for the weighting; `05` decisions 7 and 13 extend it to the rest, and `05`'s
  reset/clear table is the reason — several settings clear DSP state, which only Rust can do.

Also: the 10 Hz tick is the *meter's* cadence, not a global one. `05` decision 12 explicitly leaves
the spectrogram free to run at its own rate, and the internal DSP tick cannot go below 10 Hz for
reasons unrelated to display.

**Inherited from ticket [`06`](06-calibration-model.md) — three additions, one of which means the
meter payload is no longer the only meter traffic.**

- **A second level reading, for the calibration view.** `06` decision 3 computes the offset against a
  **fixed 10 s slice of the ring**, independent of the display's window setting, so the calibration
  surface needs that value *and its coverage* alongside the main L_eq. It is only needed while that
  view is open, so whether it rides the same tick payload, a separate event, or a pull command is
  this ticket's call — but the design has to have somewhere for it.
- **An uncalibrated flag.** `06` decision 4 models the offset as `Option<f64>`, and with `None` the
  frontend labels the numbers **`dBFS`** instead of `dB` — a correctly-named different quantity rather
  than a wrong SPL. So the state has to cross the bridge; the frontend cannot infer it from the
  values.
- **Two commands:** set-calibration-from-reference (taking the value typed off the proper meter, which
  Rust bounds-checks to ~0–140 dB before deriving the offset) and set-offset-directly (for retyping a
  written-down offset after a reinstall). Both are settings commands in the same family as the other
  four, and both write through to disk immediately.

Note `06` decision 5 also settles the storage side: **one JSON file written from Rust** via
`app_config_dir()`, no plugin and no capability entry, which keeps the frontend out of persistence
entirely. Whatever this ticket designs, the frontend should never be the thing that remembers.

**Inherited from ticket [`07`](07-spectrogram-form.md) — the bulk payload this ticket was braced for
does not exist.**

`07` settled the spectrogram on **32 fixed one-third-octave bands**, so a column is **32 band levels,
not 4 096 magnitude bins**: 128 bytes at 10 Hz, ≈**1.3 kB/s**, the same order as the meter's own
scalars. The question "where does downsampling happen" is therefore answered — **in Rust, before the
bridge** — and it is *banding* rather than downsampling, which also means the frontend never sees a raw
bin and cannot accidentally invent a level from one. What is left for this ticket is genuinely small:

- **Gap columns must be marked**, not merely absent. `07` decision 11 draws a gap as a visible hole
  rather than a low level, on the same reasoning as `05` decision 11's `--`, so the wire format needs a
  way to say "no data for this column" that is distinct from "quiet".
- **Band levels carry the calibration offset**, applied post-log in Rust exactly as the meter's numbers
  are (`05` decision 13). That is what makes a colour mean an absolute level, and it means the
  uncalibrated flag governs the spectrogram's legend as well as the numbers.
- **The span follows the window setting** (`07` decision 8, 10/30/60/120 s). Either Rust re-slices its
  own column ring or the frontend keeps a 120 s ring of its own — a design choice this ticket owns, no
  longer a question about volume.
- **Columns are aggregated by energy into the pixel budget, never decimated by the resampler**
  (`07` decision 6). Whether that aggregation happens in Rust (knowing the pixel width) or in the
  frontend (knowing its own canvas) is this ticket's call; `07` only insists it is deliberate.

Also settled upstream and worth not re-deriving: the FFT is **10 columns/s, one per `05` ring slot**, and
it feeds nothing but the picture, so its cadence is independent of the meter's 10 Hz tick even though
they happen to match.

**Inherited from ticket [`04`](04-weighting-architecture.md).** The traffic is now known to be
lopsided in exactly the way this ticket's last paragraph guessed: `04` chose two pipelines, so the
meter crosses as **a scalar plus window coverage per display frame** — trivial — and **FFT
magnitude frames are the only bulk payload**. Get the spectrogram's path right and the boundary is
right.

Also settled upstream: the weighting mode (C/A/Z) is a **settings-page** value, and changing it
resets the rolling window (`04` decision 2, `05`). So the "who owns settings" question has at least
one answer already implied — a weighting change is a command that reconfigures the DSP, not a view
preference the frontend can hold privately.

## Question

Audio capture and DSP live in Rust; the display lives in Vue. What crosses the bridge?

- **Events or commands?** Tauri events push from Rust; commands pull from the frontend.
  The meter and the spectrogram may want different answers.
- **Payload shape and rate.** Three scalar numbers per display frame is trivial. Full FFT
  frames at display rate is the obvious way to make this app feel bad — serialising and
  copying magnitude arrays 30–60 times a second through the webview bridge.
- **Where downsampling for display happens** — Rust side or Vue side.
- **What state is authoritative where**: does Rust own the calibration offset, weighting
  choice, and window length, with the frontend as a view? Or does the frontend own
  settings and push them down?

The spectrogram is the demanding case and the meter is the easy one; a design that only
considers the meter will need redoing.

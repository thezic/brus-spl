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

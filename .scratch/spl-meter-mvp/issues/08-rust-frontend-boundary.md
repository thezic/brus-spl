# Rust ↔ frontend boundary: what crosses, how often, in what shape

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open
Blocked by: 05

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

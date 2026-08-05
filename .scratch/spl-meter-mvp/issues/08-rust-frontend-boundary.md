# Rust ↔ frontend boundary: what crosses, how often, in what shape

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open
Blocked by: 05

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

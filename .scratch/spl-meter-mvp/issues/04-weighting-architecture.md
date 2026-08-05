# Weighting architecture: time-domain biquads or FFT-domain?

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open — unblocked, on the frontier
Blocked by: 01, 02, 03  (all resolved)

**Hard numbers now available from `02`, measured on the device rather than assumed:**
**48 000 Hz, mono, f32, 1024-frame buffer** (`IOBufferDuration` 0.021333 s), block rate
46.875/s. So FFT sizing, bin resolution and biquad coefficients can be argued concretely.
Note 48 kHz clears research `03`'s ~40 kHz minimum — but a Bluetooth route may drop it to
8–16 kHz, which ticket [`11`](11-interruption-and-gap-handling.md) is probing; the answer here
should not assume 48 kHz is guaranteed for all routes.

## Question

The spectrogram needs an FFT regardless. So weighted levels could be derived from that
same FFT — apply the weighting curve to the magnitude bins and sum — instead of running a
separate biquad chain over the time-domain samples.

- **One pipeline or two?** FFT-domain weighting means a single analysis path feeding both
  the meter and the spectrogram. Biquads mean two independent paths, but each does its own
  job properly.
- **Accuracy.** FFT-domain weighting is bin-resolution-limited and window-dependent, which
  matters most at low frequencies — exactly where dB(C) lives. Biquads have no such
  problem.
- **What the standard assumes.** IEC 61672 is written around continuous filtering; L_eq
  from summed FFT bins is a different computation that happens to approximate it.

This decision shapes tickets `05`, `07`, and `08`, so it must be settled before any of
them.

## Evidence from ticket 03

Research measured the comparison rather than reasoning about it — see
[`research/03-iec-weighting-filters.md`](../research/03-iec-weighting-filters.md) §7. The
result is one-sided at low frequencies, which is where dB(C) lives:

| Condition | FFT-domain error vs biquads |
|---|---|
| Clean content | ~0.02 dB — indistinguishable |
| 12 Hz component present, N=1024 | **C over-reads by +6.1 dB** |
| 12 Hz component present, N=2048 | **C over-reads by +4.3 dB** |
| 12 Hz component present, N=8192 (170 ms) | under 0.6 dB |
| 30 Hz tone, rectangular window | **A over-reads by +2.4 dB** |

The error direction is **over-reading**, meaning false alarms against the 70 dB ceiling —
the app would tell Simon to turn a talk down when it didn't need turning down. And a venue
with HVAC is exactly the environment that supplies the infrasonic content that triggers it.

**Recommended answer: biquads for the reported level, FFT for the spectrogram only.** Two
pipelines, not one. The "one pipeline is simpler" argument loses because the shared pipeline
would have to run N≥8192 to be trustworthy, which is a 170 ms frame — sluggish for the live
instantaneous readout — and it would still be the less defensible of the two.

Two facts that also land here:

- **Minimum viable sample rate is ~40 kHz** (36.9 kHz for class-1 limits, 39.0 kHz for the
  design band). Below that the filter is out of tolerance regardless of topology. A runtime
  guard is needed; the Bluetooth-route case is owned by ticket
  [`11`](11-interruption-and-gap-handling.md).
- **Sample rate is a runtime value** read back from the audio session (ticket
  [`01`](01-native-audio-capture-path.md)), so coefficients are computed at startup, not
  compiled in. Research measured this at ~30 flops.

What is left for this ticket is therefore mostly confirmation rather than open exploration —
unless Simon wants to trade defensibility for a single pipeline, which he should be told is
the trade rather than discovering it later.

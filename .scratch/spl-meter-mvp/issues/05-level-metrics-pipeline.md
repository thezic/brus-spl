# Level metrics pipeline: rolling L_eq, instantaneous level, max hold

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open — unblocked, on the frontier
Blocked by: —  (was 04, now resolved)

**Inherited from ticket [`04`](04-weighting-architecture.md).** The input to this pipeline is now
defined: **one** weighting chain runs at a time (the mode selected on the settings page — C, A or
Z), cascaded f64 biquads in the audio callback, and the accumulator averages `p_weighted²`
**linearly with no time weighting inside it** (clause 3.9 NOTE 3 — F/S belong to L_AF/L_AS, not
L_Aeq). Smooth the *displayed* number if the instantaneous readout needs it; keep the accumulator
plain.

Two consequences for the window specifically:

- **It has a second reset cause** besides the manual reset button: **changing the weighting
  setting**. LCeq and LAeq are different quantities and cannot be averaged together, so prior
  energy is incommensurable and the window restarts from empty. The filter's settling transient is
  buried by this, so it needs no separate handling.
- **A sample-rate change is *not* a reset** — it is gap slots, per `11` decision 3. Same quantity,
  differently-designed filter, so the old slots remain valid energy.

**Inherited from ticket [`11`](11-interruption-and-gap-handling.md) — a hard requirement not in
the original list.** The rolling L_eq must track **coverage**, not only energy: `11` decided the
display shows the level *and* how much of the window is real data (`33s of 60s`). So the ring
buffer has to know which slots hold actual samples rather than merely accumulating into them.
This is not cosmetic — a measured interruption produced a 27 s hole in a 30 s run with no error
of any kind, and without coverage tracking the average would have looked entirely plausible while
covering a tenth of the time it claimed.

Also from `11`: capture is **foreground-only** and does not survive backgrounding, so the window
never has to span an app suspension — but the idle timer must be disabled, which is why it can be
assumed continuous while running.

Hard numbers from `02`, measured: **48 000 Hz, mono, f32, 1024-frame buffer** (21.33 ms per
block, 46.875 blocks/s). Useful for choosing the accumulation granularity.

## Question

How each of the three numbers on screen is actually computed.

- **Rolling L_eq** over a configurable window (default 60 s). How is "rolling"
  implemented — a ring buffer of energy accumulations, at what granularity? What happens
  in the first 60 seconds after launch, before the window is full?
- **Instantaneous level.** Which time weighting — Fast (125 ms) or Slow (1 s)? This is the
  number that makes the meter feel alive or feel broken.
- **Max hold.** What does it hold: the peak instantaneous level, or the peak of the
  rolling L_eq? What exactly does the reset button clear?
- **Display update rate**, and where it's decoupled from the audio callback rate.
- **Where the configurable window length lives** in the model, and its allowed range.

Constraint from charting: no warnings, no automatic resets. Simon reads the numbers and
decides.

Two constraints inherited from ticket [`01`](01-native-audio-capture-path.md):

- **The sample rate is a runtime value**, read back from the audio session — not a
  compile-time constant. The window arithmetic has to be derived from it.
- **The sample stream can contain silent gaps** after an audio-session interruption. How
  the metrics respond to that is ticket [`11`](11-interruption-and-gap-handling.md), but the
  pipeline designed here has to have somewhere for that answer to live.

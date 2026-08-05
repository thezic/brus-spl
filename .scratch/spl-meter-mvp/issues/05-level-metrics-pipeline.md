# Level metrics pipeline: rolling L_eq, instantaneous level, max hold

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: open
Blocked by: 04

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

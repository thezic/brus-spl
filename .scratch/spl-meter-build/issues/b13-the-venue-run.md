# The venue run

Parent: [SPL Meter Build](../map.md)
Type: task
Status: open
Blocked by: [`b12`](b12-re-sign-and-install-rehearsal.md)

## The work

**This is the destination.** The map is done when this ticket is closed — not when the code
matches the spec.

It is also the first time this design meets a real microphone in a real room. Every number in
the spec that came from a prototype, a synthetic source or a scripted probe gets its first
honest test here.

### Calibrate, per spec §8.4

In order, and the order matters:

1. Set the weighting to **dB(C)** on the sheet — or to whatever the reference meter is showing.
2. Put the phone and the reference meter **in the same place, pointed at the same steady
   sound.** **Steady is load-bearing, not advisory**: the two instruments may not be reporting
   the same quantity, so if it shows an F-weighted live level and the app shows a 10 s L_eq,
   they agree only when the sound is not changing.
3. Wait for the calibration reading to fill its 10 s slice. Coverage is displayed — judge it.
4. Read the reference meter and **type its value**. The app stores `reference − raw slice`, and
   every displayed number moves at once, including the settled 60 s average and the max hold.
5. Trim ±0.1 dB if the two disagree slightly.
6. **Write the offset down.**

Expect an offset on the order of **+100 dB**. If it comes out near zero or in the hundreds,
something is wrong upstream — most likely `Measurement` mode failed to apply, which is worth
~21 dB (§13.9).

### Read a talk

Then use it. Reset at the start of the talk — that is what the button is for, "start measuring
this talk" — and read it as you would.

### What to bring back

The map's **Not yet specified** section exists for this ticket to clear. Each of these has a
settled parameter waiting on the answer:

- **Does `−90 … −30 dBFS` fit real speech in a real room?** §7.1 names the colour window as the
  value most likely to want moving. (Only if [the canvas](b11-the-spectrogram-canvas.md)
  landed.)
- **Does the hero number feel right on real speech at `S`?** If it reads as too busy, §11.4
  names the escape: per-quantity display resolution, tenths on the L_eq and something coarser
  on the live value — deliberately not taken, and available.
- **Arm's-length legibility in a dim venue.** ~6 px per band, and the muted `--` at hero size.
  A "no" is a correction to the band count or the ~200 px height.
- **The accepted risk in §11.1**: the number that dominates the screen is not the number judged
  against the 70 dB ceiling. Does that bite in practice, or is having the L_eq permanently
  beside it enough?
- **Did anything interrupt it**, and did it recover — or did you have to restart?
- **`get_spectrogram` on the phone**, if the picture is there: 276 KB of JSON at a 120 s span
  has never been parsed in a WKWebView.

### Two things to be honest about while reading it

- **The offset inherits the reference meter's error wholesale.** A class-2 instrument is
  ±1.5 dB while the app displays 0.1 dB. **The precision of the display is not the accuracy of
  the reading** (§13.6).
- **The offset is only valid for the input it was set on.** Plugging in a headset mic can move
  the sensitivity by tens of dB with nothing at runtime to say so — §13.4, the largest single
  risk in the design.

## Done when

The talk has been read, and the answers above are recorded in the resolution. Anything that
moves a settled parameter graduates onto the map as a fresh ticket; anything that does not is
the map closing.

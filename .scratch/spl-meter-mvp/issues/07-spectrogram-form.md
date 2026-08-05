# Spectrogram: parameters and visual form

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: open — unblocked, on the frontier
Blocked by: —  (was 04, now resolved)

**Inherited from ticket [`04`](04-weighting-architecture.md).** The FFT is now the spectrogram's
alone — it does **not** feed the reported level, so FFT size is free to be chosen for how the
display looks rather than for measurement accuracy. `04` rejected deriving the number from bins
precisely so this ticket would not be constrained to N ≥ 8192.

- **The FFT reads raw, unweighted samples**, tapped ahead of the weighting filter. So this ticket
  owns whether the display applies a weighting curve. Both are available and it is a cheap per-bin
  gain from the closed-form magnitude equation either way — safe here, because `04`'s +6.1 dB
  objection to FFT-domain weighting only applies when bins are summed into a reported number, and
  a colour map sums nothing.
- **There is no DC blocker** (`04`), so **bin 0 carries the microphone's DC bias**. Don't draw it —
  the standard's band starts at 10 Hz regardless.
- Note there are now **three** meter modes (C, A, Z), so "match the meter" is a three-way
  question if this ticket goes that way.

## Question

**First, confirm the word.** Simon said "spectrogram" — a scrolling time-frequency display,
history moving across the screen. That is different from a "spectrum" — an instantaneous
bar or line display of the current frame. Decibel X shows the latter. Ask before building.

Then, once that's settled:

- **FFT size, hop size, window function** — and how they trade time resolution against
  frequency resolution given the content is speech.
- **Frequency scale** — log or linear. Log is what matches hearing; linear is what an FFT
  hands you.
- **dB range and colour mapping** for a spectrogram, or scaling for a spectrum.
- **Time axis span** if it scrolls.

Build a rough prototype and react to it. Do not settle this on paper — it's a
"how should it look" question, which is exactly what a prototype is for.

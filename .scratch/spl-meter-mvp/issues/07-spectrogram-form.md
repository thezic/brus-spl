# Spectrogram: parameters and visual form

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: open
Blocked by: 04

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

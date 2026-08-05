# Screen layout: meter and spectrogram together

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: open
Blocked by: 07  (05 now resolved)

**Inherited from ticket [`05`](05-level-metrics-pipeline.md) — four obligations, one of them new
work rather than a constraint.**

- **The active-mode indicator is two-dimensional.** `04` narrowed the old "how does the A/C switch
  present" question to "how is the active mode indicated"; `05` decision 7 made the F/S time
  weighting selectable too, and decision 8 ties the *meaning of max hold* to both. So a bare
  `MAX 72.4 dB` is unreadable — the layout has to say which weighting and which time constant
  produced it.
- **`--` is a designed state, not an error state.** The instantaneous readout shows `--` after
  200 ms with no audio, and the L_eq shows `--` at zero coverage (`05` decision 11). This is the
  instrument reporting its own state, so it should read as deliberate rather than broken.
- **Coverage sits next to the L_eq, always** — `33s of 60s`, including `60s of 60s` when full. Not
  conditional on being degraded: an indicator that appears only when something is wrong is a
  warning, which charting ruled out, and always-on is what makes the number trusted.
- **Reset now discards the window as well as the max hold** (`05` decision 10 — it means "start
  measuring this talk"). So its placement has to resist a mis-tap in a way a max-only reset would
  not have needed.

Also useful: **display resolution is this ticket's readability lever.** `05` decision 12 publishes at
10 Hz, but a 60 s L_eq only moves hundredths of a dB per tick, so at 0.1 dB the digit changes about
once a second on its own. Choosing resolution is how the numbers are made calm — not by slowing the
data.

## Question

One screen has to hold: rolling L_eq (the number judged against 70), instantaneous level,
max hold, the spectrogram, the dB(A)/dB(C) switch, window length, the reset button, and
calibration entry.

- **What's primary?** The rolling L_eq is the number Simon acts on. It should dominate;
  everything else supports it.
- **Glanceability.** This is read at a venue, mid-work, probably from a distance and at an
  angle. Legibility beats density.
- **iPhone orientation** — portrait or landscape, and whether it needs to work in both.
- **Desktop window** — the dev loop runs there, so the layout has to survive an 800×600
  desktop window as well as a phone.
- **Where settings live** — inline on the one screen, or behind a sheet.

Prototype it. This is a "how should it look" question, and the fog note about how the
dB(A)/dB(C) switch presents should get resolved here or graduate into its own ticket.

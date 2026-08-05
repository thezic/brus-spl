# Screen layout: meter and spectrogram together

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: open
Blocked by: 05, 07

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

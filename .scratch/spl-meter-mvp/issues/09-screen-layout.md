# Screen layout: meter and spectrogram together

Parent: [SPL Meter MVP](../map.md)
Type: prototype
Status: open — unblocked, on the frontier
Blocked by: —  (07 and 08 now resolved, and 05 before them)

**Inherited from ticket [`08`](08-rust-frontend-boundary.md) — one new thing to design, and two
obligations taken away.**

- **New: an input-state line.** `08` decision 8 puts a three-value `input` field
  (`capturing` | `denied` | `unavailable`) in every tick, because a denied microphone otherwise
  renders as `--` beside `0s of 60s` forever with nothing saying why — and it is the one failure the
  user can fix, from outside the app. It must read as a **state label in the family of `dBFS`**, not
  as a warning banner: `06` decision 4's precedent, not a reversal of `11`/`05`'s no-warnings rule.
  For `denied` it should say something actionable (Settings ▸ Privacy); for `unavailable` there is
  nothing to act on, and detail lives in the log.
- **Removed: the layout holds no state.** `08` decision 7 puts the four settings *and* the unit in
  every tick, and `08` decision 4 keeps the spectrogram's history in Rust. So the frontend renders
  the last tick and caches nothing authoritative — no settings copy that can disagree with what is
  running, no column ring of its own. Text entry keeps local draft state; that is an input buffer,
  not state.
- **When to re-pull the picture:** `get_spectrogram()` on mount, on a canvas resize or orientation
  change, and on a window-length change. **Not after calibration** — `07` decision 9 shifts the
  colour window by the offset and the band values shift with it, so calibrating changes every number
  on screen and no pixel of the picture. Only the legend relabels.
- **Picker feedback comes from the command return value**, not the next tick. Every settings command
  returns the new settings, so a tap updates immediately instead of waiting up to 100 ms. If the
  controls feel sticky, that is the path to check.
- **Reset does not clear the spectrogram** (`08` decision 5). Three seconds after a Reset the number
  reads `3s of 60s` while the picture still shows the previous talk's applause — deliberate, because
  60 s of rumble stripe is unrecoverable and the number refills honestly. Both axes are labelled, so
  the layout's job is to make sure they *look* like two axes: the coverage figure belongs with the
  number, the `−60s … now` axis with the picture.

**Inherited from ticket [`07`](07-spectrogram-form.md) — the spectrogram arrives with a geometry
rather than as an unknown shape, and it brings one obligation.**

- **Wide and short: ~200 px tall at full width, sitting under the number.** `07` chose a horizontal
  frame over a vertical waterfall precisely on this ticket's behalf — the waterfall wanted 360+ px of
  the portrait height that the number, coverage, max hold, the two mode indicators and reset are all
  already competing for.
- **Three gutters are part of the design, not decoration** — ~40 px left for frequency labels, ~20 px
  bottom for the time axis, ~52 px right for the colour legend. **If space forces the legend out, the
  picture loses its absolute meaning**; that is a trade this ticket may make, but knowingly rather than
  by cropping.
- **The window length has to be visible near the picture**, because `07` decision 8 makes the
  spectrogram's span *follow* the L_eq window (10/30/60/120 s). The picture is then literally what is
  inside the number — which only reads that way if the two are seen together.
- **A gap in the picture is a hole**, drawn as background (`07` decision 11). Like `--`, it is a
  designed state rather than a rendering failure, so it should look deliberate.
- Also useful: `07`'s prototype ran at ~390 px wide and **could not be judged on real glass** (Wi-Fi
  client isolation keeps it off the phone). 32 bands over 200 px is ~6 px per band. Whether that reads
  at arm's length in a dim venue is the open question `07` handed on, and it is the same question that
  killed its "calm, no chrome" variant — so if this ticket gets to a device, that is the thing to look
  at first.

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

**Inherited from ticket [`06`](06-calibration-model.md) — a calibration surface this ticket did not
know it owned, plus one change to the main screen.**

The calibration surface needs four things, and the list is short because `06` deliberately kept the
gesture to numeric entry:

- **Numeric entry for the reference value** — you type what the proper meter reads and the app derives
  the offset. This is the primary gesture, done standing next to the reference meter, so it wants to be
  quick to reach and hard to fumble.
- **The live 10 s reading with its coverage**, which is what the typed value is matched against (`06`
  decision 3). It settles in 10 s and it is *not* the main display's window, so it is a distinct number
  on this surface.
- **±0.1 dB trim buttons**, for splitting the difference while watching both instruments.
- **The offset shown as a number.** Not decoration: free provisioning means the app is reinstalled
  weekly, and if the data container is ever lost, a written-down offset is the difference between a
  retype and a trip back to the venue (`06` decision 2).

And on the main screen: **the unit label switches between `dBFS` and `dB`.** `06` decision 4 makes
uncalibrated a real state — offset `Option<f64>`, `None` meaning never calibrated — and the honest
presentation is to label the raw values as the different quantity they are rather than blank them or
fake an SPL. So the layout needs both labels to look deliberate, not just the calibrated one.

Where the calibration surface *lives* is still this ticket's call — the layout question already lists
"inline on the one screen, or behind a sheet", and calibration is now the strongest case for a sheet:
it is a rare, deliberate, two-instrument gesture, unlike anything else on the screen.

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

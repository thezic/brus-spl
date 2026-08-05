# Level metrics pipeline: rolling L_eq, instantaneous level, max hold

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: resolved
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

---

## Answer

**A time-indexed ring of 100 ms energy slots, advanced by the monotonic clock rather than by
arriving samples — which makes gap accounting a consequence of timekeeping instead of a feature.**
The L_eq averages only real data, so the first 60 seconds after launch and a 27-second hole are
the same case, both reported as `33s of 60s`. One thread boundary: the audio callback pushes one
summary per block into a bounded queue, and everything else happens on a 10 Hz tick.

```
audio callback (real-time)                     display tick (10 Hz)
──────────────────────────                     ────────────────────
f32 → f64 at the block boundary                drain the queue; per block:
selected chain  C:2  A:3  Z:bypass               ├─ deposit (Σp², n) in the clock-current slot
Σp² over the block, plus n                       ├─ advance the F/S smoother by n/fs
push {Σp², n, t} ── bounded SPSC ────────→       └─ max hold compare      (46.875 /s)
                                               re-sum the ring → L_eq, coverage
                                               + calibration offset, in dB
                                               publish 3 × Option<f64> + coverage
```

### Decisions (2026-08-05, with Simon)

**1. The L_eq averages real data only — `Σp² / Σ actual samples`, never `/ nominal window`.**

Ticket `11` decision 3 settled that the display carries the level *and* the coverage, but left the
arithmetic open. Folding a gap in as silence costs **−2.6 dB** for a 27 s hole in a 60 s window,
and the direction is *under*-reading against a ceiling — the number would move with the shape of
the hole rather than with the room. Averaging over real data makes the displayed figure mean "the
L_eq of the 33 s I actually have", which is exactly what the coverage figure then qualifies.

**The first 60 seconds stops being a special case.** A filling window is a window with low
coverage, so startup needs no separate code path, no blank screen, and no "warming up" state. The
meter is live and honest from the first tick. This unification is the main reason the decision is
worth this much text.

**2. The monotonic clock advances the ring — not sample arrival.**

Probe 2 in ticket `11` answered this by accident. The stream died at 3.26 s and never returned,
with `stream errors: none`. A sample-driven ring stops moving when the samples stop, so it would
have gone on reporting its last state at **full coverage** over a stream that ended half a minute
earlier — a confident lie, and precisely the failure `11` decision 3 was built to prevent.

With the clock advancing it and the callback merely depositing into whichever slot is current,
**slots nobody deposited into are gap slots by construction**. The property worth noticing: gap
accounting becomes a consequence of timekeeping rather than a feature, so it catches missing data
*whatever* the cause — interruption, a route change that kills the stream, the dead air during a
rebuild, probe 3's deliberately-unrun case — without enumerating causes. Given probe 2 proved the
error callback stays silent, any design that only accounts for gaps it *detects* will eventually
show full coverage over a hole.

Ticket `11`'s interruption observer is still required. It is needed for **recovery**
(`setActive(true)`), not for accounting.

**3. One slot is 100 ms, holding `(sum_sq: f64, n: u32)`.**

Decision 2 rules out the tempting option of one slot per callback block: a gap emits **no blocks
at all**, so a block-indexed ring has nowhere to record it. A time grid cannot be indexed by an
event that stops happening. (The independent reason still holds too — the block is 21.333 ms *at
48 kHz*, a device quantity, so block-slots would rescale the history behind the window on a route
change.)

Given a time grid, the period is chosen for retirement smoothness. At 100 ms a retiring slot is
1/600 of the energy at a 60 s window; at 1 s slots it is 1/60, and on a 10 s window 1/10, which
stair-steps visibly once a second.

Three structural consequences:

- **Sum-of-squares *and* count, not a mean.** Decision 1 divides by the actual sample total, and
  storing a mean would throw away the weight.
- **Blocks are assigned whole** to whichever slot is current when drained. Placement is therefore
  approximate to ±21 ms while the coverage arithmetic stays **exact**, because it counts samples
  rather than slots.
- **Re-sum the window every tick** rather than maintaining a running sum by add-and-subtract. 600
  f64 adds at 10 Hz is 6 000 adds/s — nothing — and it removes any question of drift from
  add/subtract asymmetry over an eight-hour session. Coverage comes out of the same loop free.

**4. A mid-session sample-rate change is still not a window reset. Ticket `04` stands.**

Reopened by Simon, on the ground that a route change is a deliberate act after which nobody expects
data to be retained — the same reasoning he used for `11` decisions 5 and 6. Investigated properly
and **re-confirmed as no reset**, for three reasons:

- **The sample rate is the wrong trigger.** A wired headset can supply a different microphone at
  the *same* 48 kHz, so a rate-change reset misses it; and iOS can move the session rate for
  reasons unconnected to the input device, so it fires when nothing changed.
- **The right trigger reopens a closed ticket.** Route/device identity is what actually means
  "different instrument", and `11` decision 6 deliberately declined to detect it. Building that
  detection to reset a window would also make decision 6 indefensible — knowing the mic changed
  and silently keeping a wrong offset is a far harder position than never looking.
- **The prize is at most one window length.** Because energy averaging is dominated by its loudest
  contributor, stale slots from a quieter previous route wash out immediately; the visible case is
  stale-loud-into-new-quiet, and it retires within the window regardless. So an automatic reset
  buys ≤60 s of correctness, on an action just taken deliberately, with Reset already under thumb —
  for a problem whose *permanent* version (`11` decision 6, the offset wrong by tens of dB) is
  already accepted and sent to the spec.

One thing did get cheaper: `04` asked for the transition to be "marked as gap slots", and under
decision 2 that is **zero code**. A rebuild delivers no samples, so those slots are gaps already.

**5. Changing the window length re-slices the ring. No reset.**

`04` resets the window on a weighting change because LCeq and LAeq are **incommensurable**. A
length change is the opposite case — same weighting, same quantity, and the energy in the ring is
valid at any length. So the ring is allocated at the **longest permitted window, always**, and the
setting is nothing but *how far back do I sum*.

Consequences: 60 s → 120 s is instant and uses data already held (`60s of 120s`, a genuine 120 s
average of everything there is, rather than starting from empty); shortening is instant and exact;
and **no third reset cause is created**. Cost is 1 200 slots ≈ 19 KB, permanently.

The rejected alternative — resetting for consistency with the weighting change — mistakes surface
symmetry for the real test. `04`'s asymmetry was about commensurability, and by that test a length
change belongs with the rate change, not with the weighting change.

**6. Selectable lengths: 10 / 30 / 60 / 120 s, default 60. A picker, not a numeric field.**

The short end has a job beyond impatience: ticket `06`'s calibration gesture is matching a
reference meter on a steady sound, which wants a number steadier than the instantaneous readout but
settling in seconds. A 10 s L_eq is that number, and calibration is not speech, so its jumpiness on
speech does not count against it. The long end now sets only the ring size (decision 5), and 120 s
stops well short of the blindness that got 15 minutes rejected while charting. A free numeric entry
was rejected as a keyboard on a phone at a venue, plus a moving-target maximum, for no gain.

**7. Fast/Slow is selectable on the settings page. Simon's call, against the recommendation.**

Recommended F-only, on two grounds: with a 10 s window now available (decision 6) an S-weighted
readout at τ = 1 s occupies nearly the same perceptual slot as a short L_eq — two slow numbers and
nothing live — and F is the only reading in the design that responds inside a syllable, which is
what this ticket meant by "the number that makes the meter feel alive or feel broken". Overruled;
it is one coefficient.

The mechanics are clause 3.5 Figure 1 (research `03` §6): a one-pole low-pass on the **squared**
weighted pressure, `y += (x² − y)(1 − e^{−dt/τ})`, then to dB. What the setting does and does not
touch:

- **The L_eq accumulator: untouched.** Clause 3.9 NOTE 3 keeps time weighting out of L_eq
  entirely, so this setting never reaches the ring. No reset, not even a flicker.
- **The smoother: no reset.** Its state is a smoothed mean square either way, so a τ change swaps
  the coefficient and keeps the state; it re-converges in a few hundred ms going S→F and a couple
  of seconds F→S. No artefact worth handling.
- **Max hold: see decision 8.** This is the one place the setting bites.

**8. Changing F/S clears the max hold. So does changing the weighting — a correction to `04`.**

L_CFmax and L_CSmax are different quantities; S-max sits several dB below F-max on speech, because
the transients are what the 1 s pole flattens. A maximum captured under one and read under the
other is the same incommensurability `04` invoked for the window, and honouring it costs one
scalar.

**`04` only said the *window* resets on a weighting change; it did not consider the max.** L_CFmax
and L_AFmax are not comparable either, so the max must clear there too. Recorded here as a
correction rather than left implicit.

**9. Max hold holds the maximum time-weighted level — L_CFmax (clause 3.6). Tracked at block rate.**

The two alternatives got a fair hearing. **Max of the rolling L_eq** is the compliance-shaped
figure if the imposed 70 dB is an LCeq limit, and that argument is real — but it is lagged by up to
a window length, so you learn about a bad minute a minute after you could have acted, and under
decision 1 it would latch onto whatever the L_eq read at low coverage, letting a 2-second average
set the session maximum. Fixable with a coverage gate, which is machinery. **True peak** (L_Cpeak,
clause 3.8) is the impulse quantity, is the only one of the three that must run per-sample in the
audio callback, and against a 70 dB speech ceiling mostly reports chair scrapes.

Implementation requirement, not optional: the maximum is compared **inside the drain loop at block
rate** (46.875 /s), not sampled from the smoother at the 10 Hz display tick, which would drop four
peaks in five.

**10. One Reset button, clearing the max hold *and* the window.**

The map's wording ("a max hold cleared by a manual reset button") reads as max-only, and the window
is self-clearing anyway — so the question is whether waiting a window length is ever too long. It
is, in the actual use case: between talks the room is not quiet, and applause, chatter, pack-up and
PA music sit in the window through the **first full minute of the next talk**, reading high against
the ceiling exactly when the new speaker is being judged. One gesture clears it, and under decision
1 the refill is honest rather than blank (`12s of 60s` climbing), so resetting costs nothing in
trust. The button is not "clear the max", it is **"start measuring this talk"**. Two buttons would
ask Simon to care about a distinction that does not arise at a venue.

**11. When no samples arrive, publish no value — `--` — rather than a guess.**

The last place the design could still lie, and it needed deciding because the smoother's
no-input behaviour is a choice, not a default. Fed **zeros**, the readout decays smoothly toward
−∞ dBFS, which on screen is indistinguishable from *the room went quiet* — a plausible, wrong,
unlabelled number, and worse than the L_eq case because it carries no coverage figure beside it.
**Frozen**, it is stale with no tell. **Absent**, it is the instrument reporting its own state, the
same move as showing coverage.

Threshold: **no block for 200 ms** — about 9 missed blocks, far outside scheduling jitter, fast
enough to be honest, slow enough not to flicker.

Two consequences that are arithmetic rather than choices: at **zero** coverage the L_eq is
undefined (`Σn = 0`, the log of nothing), so it publishes no value too — `--` beside `0s of 60s`.
And **max hold is unaffected**; a hold is historical by nature, so it simply stops rising.

**12. One publish at 10 Hz for all three numbers. The audio callback is decoupled by a bounded
queue of per-block summaries.**

The audio callback does the minimum: f32 → f64 at the block boundary, the selected chain per sample
(15 mul-adds worst case, `04` decision 1), accumulate `Σp²` and `n`, and push **one summary per
block** — `{sum_sq, n, timestamp}` — into a bounded lock-free SPSC queue. No locks, no allocation.
Everything else happens on the tick. Two properties worth having:

- Advancing the smoother **per block by that block's own `dt`** makes it mathematically identical
  to advancing it continuously, however irregularly the tick fires. Combined with the block-rate
  max comparison (decision 9), the tick's timing affects only what is *painted*, never what is
  *measured*.
- Size the queue at ~1 s of blocks and an overflow from a stalled UI thread **degrades into lost
  coverage, not a wrong number**. The failure mode lands in the honest column by construction.

So there is one thread boundary and no separate metrics thread.

**On the repaint rate specifically** — Simon asked whether the L_eq could repaint every 1/5/10 s
instead, since it always shows the last 60 s either way. It could, but it needs nothing. One tick
swaps one slot out of 600, so at a 60 s window:

| the arriving 100 ms slot | L_eq moves by |
|---|---|
| near the window average | ~0.00 dB |
| 10 dB above it | 0.065 dB |
| 20 dB above it (a shout, a dropped chair) | 0.66 dB |

At 0.1 dB display resolution the digit therefore changes about once a second **on its own**, and
jumps a few tenths only when something genuinely happened. The calm number is the physics of a
600-slot average, not a refresh policy. Slowing the publish would meanwhile break it in three
places: the 10 s window has 100 slots and is *supposed* to be lively (the same 20 dB slot moves it
~3 dB), coverage climbs 1 s per second so a 10 s publish would show `33s of 60s` when it is really
43 — making the honesty indicator stale, which is a bit much — and after a Reset the display would
sit on the old number until the next publish unless special-cased.

Note the internal tick cannot slow below 10 Hz regardless: decision 9 needs the drain for
block-rate max tracking, and decision 11's 200 ms threshold needs it too.

**13. The calibration offset is added post-log, in Rust, to each published dB value.**

Ticket `06` owns the offset; `05` owns where it lands, and the choice shapes `06`'s procedure. As a
**gain before squaring** the ring holds calibrated energy — but then changing the offset makes
everything already accumulated belong to the old calibration, so it becomes a third reset cause,
firing at the worst possible moment: the calibration gesture *is* repeatedly nudging the offset
while watching the number against the reference meter, so every nudge would blank the window and
force a refill before the next comparison.

Added **after the log** the number is identical — `10·log₁₀(Σp²/n) + c` is exactly
`10·log₁₀(Σ(g·p)²/n)` for `c = 20·log₁₀ g`, and `max(xᵢ + c) = max(xᵢ) + c`, so the historical
maximum shifts correctly too. Nothing resets, and **the calibration gesture becomes interactive**:
nudge, and the settled 60 s average moves instantly. Kept on the **Rust** side rather than in Vue
so an uncalibrated number can never cross the bridge and get rendered by mistake — a footgun
denied to ticket `08`.

### What clears what

A table rather than a sentence, because there are now three pieces of state and six events:

| | window | max hold | filter state |
|---|---|---|---|
| Reset button | ✓ | ✓ | — |
| Weighting change (C/A/Z) | ✓ | ✓ | zeroed |
| F/S change | — | ✓ | — |
| Window length change | — | — | — |
| Sample-rate change | — | — | zeroed |
| Calibration offset change | — | — | — |

### Residual risks, stated plainly

- **The 200 ms staleness threshold is reasoned, not measured.** iOS drain jitter was never
  characterised, so if `--` flickers in practice, that number is the dial. Cheap to verify during
  implementation.
- **No coverage floor on the L_eq.** `68.2 dB · 1s of 60s` will display. Honest, but it is a
  one-second average wearing a sixty-second label, and only the coverage figure says so. Accepted
  deliberately, consistent with `11` decision 3 — the instrument reports its state and Simon
  judges.
- **Decision 7 means two settings now change what max hold means**, and neither is visible in the
  number itself. This rests entirely on ticket `09` labelling the active mode in both dimensions; a
  bare `MAX 72.4 dB` is unreadable when C/A/Z and F/S are both in play.
- **Decision 10 makes a mis-tapped Reset cost the window**, not just the max. Mitigation is
  placement and accident-resistance, which is ticket `09`'s.

### What this ticket hands downstream

- **Ticket [`06`](06-calibration-model.md) — unblocked, and its procedure gets easier.** Decision
  13 puts the offset post-log, so nudging it moves the *settled* average instantly with no reset —
  the matching gesture is interactive rather than a wait-and-see loop. Decision 6 also supplies a
  **10 s window** chosen partly for this gesture. And its persistence question now covers **four**
  settings rather than one: weighting (C/A/Z), time weighting (F/S), window length, and the offset.
- **Ticket [`08`](08-rust-frontend-boundary.md) — unblocked, and the meter half is now fully
  specified.** Per 10 Hz tick the payload is three `Option<f64>` (L_eq, instantaneous, max hold) —
  `None` meaning `--`, per decision 11 — plus coverage in seconds and the window length in seconds.
  All values are **already calibrated** (decision 13), so no uncalibrated number crosses the
  bridge. All four settings are **Rust-owned**: the frontend issues commands, it does not hold
  state. The spectrogram remains the only demanding traffic.
- **Ticket [`09`](09-screen-layout.md) — three new obligations.** The active-mode indicator
  inherited from `04` is now **two-dimensional**, C/A/Z *and* F/S, or max hold is unreadable. The
  `--` state must be designable for the instantaneous readout and for the L_eq at zero coverage.
  And Reset now discards the window (decision 10), so its placement has to resist a mis-tap.
  Display **resolution** is also `09`'s lever for readability — 0.1 dB is what makes a 10 Hz L_eq
  publish look like a once-a-second number.
- **Ticket [`10`](10-write-the-spec.md) — four settings, not three,** and the reset/clear table
  above belongs in the spec verbatim; several of its rows will otherwise read as bugs. Worth stating
  too that coverage is shown **always**, not only when degraded — a figure that appears only when
  something is wrong is a warning, which charting ruled out, and always-on is what makes the number
  trusted.

### Corrections to earlier tickets

- **Ticket `04`, "recorded not debated": the max hold also clears on a weighting change.** `04`
  addressed only the window. See decision 8.
- **Ticket `04`, same section: "mark the transition as gap slots" on a rate change is now zero
  code.** Under decision 2 a rebuild delivers no samples, so those slots are gaps by construction.

### Findings for the map's open questions

- **The coverage requirement graduated from `11` is now absorbed, not merely assigned.** The map
  listed "the rolling L_eq must track coverage, not just energy" as a requirement belonging to this
  ticket. Decisions 1–3 discharge it, and in a stronger form than `11` asked for: coverage is not a
  feature bolted onto the ring but a consequence of clock-advancing it, so it reports missing data
  from causes we never detect.
- **"How the dB(A)/dB(C) switch presents" — one more dimension.** `04` narrowed this to "how the
  active mode is indicated on the main screen". Decision 7 makes it **two** modes to indicate,
  weighting and time weighting, because decision 8 ties the meaning of max hold to both.

# Rust ↔ frontend boundary: what crosses, how often, in what shape

Parent: [SPL Meter MVP](../map.md)
Type: grilling
Status: resolved
Blocked by: —  (was 05, now resolved)

**Inherited from ticket [`05`](05-level-metrics-pipeline.md) — the meter half of this boundary is
now fully specified, so only the spectrogram is left to design.**

Per 10 Hz display tick, the meter payload is exactly:

- **three `Option<f64>`** — rolling L_eq, instantaneous level, max hold. `None` means the display
  shows `--`, and it is a real state rather than an error: the instantaneous readout publishes
  `None` after 200 ms with no audio block, and the L_eq publishes `None` at zero coverage where it
  is arithmetically undefined (`05` decision 11).
- **coverage in seconds**, and the window length in seconds — `33s of 60s`. Published **always**,
  not only when degraded; a figure that appears only when something is wrong is a warning, which
  charting ruled out.

Two constraints on the design:

- **Every dB value crossing the bridge is already calibrated** (`05` decision 13 applies the offset
  post-log in Rust). Deliberate, so that an uncalibrated number cannot be rendered by mistake. Do
  not move the offset to the frontend for convenience.
- **All four settings are Rust-owned** — weighting (C/A/Z), time weighting (F/S), window length, and
  the calibration offset. The frontend issues commands; it holds no authoritative state. `04` had
  already implied this for the weighting; `05` decisions 7 and 13 extend it to the rest, and `05`'s
  reset/clear table is the reason — several settings clear DSP state, which only Rust can do.

Also: the 10 Hz tick is the *meter's* cadence, not a global one. `05` decision 12 explicitly leaves
the spectrogram free to run at its own rate, and the internal DSP tick cannot go below 10 Hz for
reasons unrelated to display.

**Inherited from ticket [`06`](06-calibration-model.md) — three additions, one of which means the
meter payload is no longer the only meter traffic.**

- **A second level reading, for the calibration view.** `06` decision 3 computes the offset against a
  **fixed 10 s slice of the ring**, independent of the display's window setting, so the calibration
  surface needs that value *and its coverage* alongside the main L_eq. It is only needed while that
  view is open, so whether it rides the same tick payload, a separate event, or a pull command is
  this ticket's call — but the design has to have somewhere for it.
- **An uncalibrated flag.** `06` decision 4 models the offset as `Option<f64>`, and with `None` the
  frontend labels the numbers **`dBFS`** instead of `dB` — a correctly-named different quantity rather
  than a wrong SPL. So the state has to cross the bridge; the frontend cannot infer it from the
  values.
- **Two commands:** set-calibration-from-reference (taking the value typed off the proper meter, which
  Rust bounds-checks to ~0–140 dB before deriving the offset) and set-offset-directly (for retyping a
  written-down offset after a reinstall). Both are settings commands in the same family as the other
  four, and both write through to disk immediately.

Note `06` decision 5 also settles the storage side: **one JSON file written from Rust** via
`app_config_dir()`, no plugin and no capability entry, which keeps the frontend out of persistence
entirely. Whatever this ticket designs, the frontend should never be the thing that remembers.

**Inherited from ticket [`07`](07-spectrogram-form.md) — the bulk payload this ticket was braced for
does not exist.**

`07` settled the spectrogram on **32 fixed one-third-octave bands**, so a column is **32 band levels,
not 4 096 magnitude bins**: 128 bytes at 10 Hz, ≈**1.3 kB/s**, the same order as the meter's own
scalars. The question "where does downsampling happen" is therefore answered — **in Rust, before the
bridge** — and it is *banding* rather than downsampling, which also means the frontend never sees a raw
bin and cannot accidentally invent a level from one. What is left for this ticket is genuinely small:

- **Gap columns must be marked**, not merely absent. `07` decision 11 draws a gap as a visible hole
  rather than a low level, on the same reasoning as `05` decision 11's `--`, so the wire format needs a
  way to say "no data for this column" that is distinct from "quiet".
- **Band levels carry the calibration offset**, applied post-log in Rust exactly as the meter's numbers
  are (`05` decision 13). That is what makes a colour mean an absolute level, and it means the
  uncalibrated flag governs the spectrogram's legend as well as the numbers.
- **The span follows the window setting** (`07` decision 8, 10/30/60/120 s). Either Rust re-slices its
  own column ring or the frontend keeps a 120 s ring of its own — a design choice this ticket owns, no
  longer a question about volume.
- **Columns are aggregated by energy into the pixel budget, never decimated by the resampler**
  (`07` decision 6). Whether that aggregation happens in Rust (knowing the pixel width) or in the
  frontend (knowing its own canvas) is this ticket's call; `07` only insists it is deliberate.

Also settled upstream and worth not re-deriving: the FFT is **10 columns/s, one per `05` ring slot**, and
it feeds nothing but the picture, so its cadence is independent of the meter's 10 Hz tick even though
they happen to match.

**Inherited from ticket [`04`](04-weighting-architecture.md).** The traffic is now known to be
lopsided in exactly the way this ticket's last paragraph guessed: `04` chose two pipelines, so the
meter crosses as **a scalar plus window coverage per display frame** — trivial — and **FFT
magnitude frames are the only bulk payload**. Get the spectrogram's path right and the boundary is
right.

Also settled upstream: the weighting mode (C/A/Z) is a **settings-page** value, and changing it
resets the rolling window (`04` decision 2, `05`). So the "who owns settings" question has at least
one answer already implied — a weighting change is a command that reconfigures the DSP, not a view
preference the frontend can hold privately.

## Question

Audio capture and DSP live in Rust; the display lives in Vue. What crosses the bridge?

- **Events or commands?** Tauri events push from Rust; commands pull from the frontend.
  The meter and the spectrogram may want different answers.
- **Payload shape and rate.** Three scalar numbers per display frame is trivial. Full FFT
  frames at display rate is the obvious way to make this app feel bad — serialising and
  copying magnitude arrays 30–60 times a second through the webview bridge.
- **Where downsampling for display happens** — Rust side or Vue side.
- **What state is authoritative where**: does Rust own the calibration offset, weighting
  choice, and window length, with the frontend as a view? Or does the frontend own
  settings and push them down?

The spectrogram is the demanding case and the meter is the easy one; a design that only
considers the meter will need redoing.

---

## Answer

**One event at 10 Hz carrying everything the screen paints, plus one command that returns the
picture's history. The frontend holds no authoritative state at all — its canvas is a view that a
single pull rebuilds.** The tick is ≈460 bytes, ≈4.6 kB/s, about 17× *under* the threshold at which
Tauri's own IPC switches to its faster bulk path — so the boundary this ticket was braced for turned
out to be a non-problem twice over: `07` deleted the bulk payload, and reading the transport deleted
the reason to invent a binary format for what was left.

```
event "tick" — Rust's own 10 Hz publish (05 decision 12), ≈460 bytes
{
  now_slot: 418752,                    // 05's clock-advanced ring index
  meter: {
    leq: 68.4 | null,                  // rolling L_eq   ┐ all calibrated,
    inst: 71.2 | null,                 // F or S         │ post-log, in Rust
    max: 74.9 | null,                  // max hold       ┘ (05 decision 13)
    coverage_s: 41.3,
    cal_leq: 68.1 | null,              // fixed 10 s slice (06 decision 3)
    cal_coverage_s: 10.0,
    input: "capturing" | "denied" | "unavailable"
  },
  settings: { weighting, time_weighting, window_s, offset_db: 101.4 | null, unit },
  columns: [ { slot: 418752, bands: [...32] } ]   // every real slot since last publish
}

commands  — all settings commands return the new Settings
  set_weighting · set_time_weighting · set_window_length
  set_calibration_from_reference(reference_db)   // Result, 0–140 bounds check
  set_calibration_offset(offset_db)              // Result
  reset()
  get_spectrogram() → Column[]    // per-slot columns for the current span
```

### What the source says (facts, not choices)

Read out of the vendored crates — `tauri 2.11.5` and `wry 0.55.1` in
`~/.cargo/registry/src/…`, the versions in `Cargo.lock`. Two of these facts removed a question
rather than answering it.

**1. `emit` and `Channel::send` are the same transport.** Both end in `webview.eval()` of a JS
source string with the JSON inlined — `src/event/mod.rs:194` (`emit_js_script`) and
`src/ipc/channel.rs:155`. At this payload size there is no throughput difference between them, so
the events-or-channels question is about **lifecycle, not speed**, which is not how the ticket posed
it.

**2. A binary format would be a pessimisation.** `Channel` only takes its faster fetch path above
**8192 bytes** of JSON; a `Raw(bytes)` payload **under 1024 bytes** is converted to a JSON *array of
numbers* and evaled (`channel.rs:163`, and the threshold constants at `channel.rs:36`). A 32-band
column is 128 bytes as f32, which would become ~400 characters of `new Uint8Array([…]).buffer` —
strictly worse than sending the numbers.

**3. Events need no capability entry.** `core:default` already includes `core:event:default`
(`allow-listen`, `allow-unlisten`, `allow-emit`, `allow-emit-to`) — the crate's
`permissions/event/`. So `CLAUDE.md`'s four-step plugin ceremony, and its warning that omitting the
capability produces a runtime "not allowed" rather than a build failure, does not apply to this
design at any point.

**4. Emitting to nobody is nearly free.** With no JS listener registered for the event,
`emit_js_filter` skips the eval entirely (`src/event/listener.rs:281`); it still serializes the
payload, and `EmitPayload::Str` can skip even that.

**5. Delivery is ordered and buffered, not dropped.** wry pushes scripts into `pending_scripts`
before the page has loaded (`wry-0.55.1/src/wkwebview/mod.rs:721`) and calls `evaluateJavaScript`
directly otherwise. A stalled webview therefore **delays** ticks rather than losing them.

**6. But the startup hole is real and the transport cannot close it.** Ticks emitted before the JS
listener is registered are skipped by fact 4, and `listen()` is itself an async round trip that
happens after page load. Nothing about picking events over channels changes this; it has to be
handled in the payload design.

**7. Command responses travel over the `ipc://localhost` custom protocol**
(`src/ipc/protocol.rs:611`), not by `eval`. So bulk belongs in a command return, and the one place
this design has bulk is exactly there.

### Decisions (2026-08-05, with Simon)

**1. Rust pushes a Tauri event on its own 10 Hz tick. Not a channel, not polling.**

Fact 1 removes the performance argument in every direction, which leaves the question of who owns
the cadence — and `05` decision 12 already answered it. That tick is not a display convenience: it
drains the SPSC queue, advances the F/S smoother by each block's own `dt`, and does the block-rate
max compare, so it **must run whether or not anyone is watching**. Pushing from it adds no second
cadence and no handshake.

The two alternatives both pay for something this design does not need. A frontend-created
`Channel` buys an explicit lifecycle (`on_drop` tells Rust nobody is listening) and a per-message
index, at the price of a `start_stream` handshake that must be re-issued after **every** webview
reload — which in the dev loop is every HMR save, and `09`'s layout work will consist of little
else. Polling a command buys real backpressure, but fact 5 says a stalled webview delays rather than
drops, so there is nothing to apply backpressure *to*; it would make the frontend own a cadence
while Rust's tick went on running anyway.

**2. One event carries the meter, the settings and the spectrogram column together.**

Coherence, not efficiency. `07` decision 5 puts the hop at 100 ms, one column per `05` ring slot,
so both halves are already 10 Hz and both derive from the *same* slot; and `07` decision 8 sells the
picture as what is inside the number. That reading only holds if the number and the column that
arrive together describe the same 100 ms. Two events could drift by a tick under scheduling jitter
with nothing to notice it. One eval per tick instead of two is a minor second benefit — each emit is
a dispatch onto the WKWebView main thread.

**3. Every column carries its absolute slot index, and each tick carries every real column since
the previous publish.**

This is the one decision that is **required for correctness** rather than chosen for robustness.
`05` decision 2 advances the ring by the monotonic clock, so if the publish tick fires late — a
timer on a phone — two or three slots have genuinely completed, and a one-column payload would drop
real data on the floor. Fact 6 adds a second case: the columns produced between page load and
`listen()` registering are lost no matter what the transport is.

Two properties fall out of it:

- **Gaps need no marker.** A slot index in `(last_drawn, now_slot]` with no column is a hole, which
  is precisely `07` decision 11's drawing rule. `05` decision 2 made gap accounting a consequence of
  timekeeping rather than a feature; this makes *drawing* the gaps a consequence of the same
  arithmetic. Explicit `bands: null` entries were considered and rejected as duplicating what the
  indices already say, at 270 wasted entries for a 27 s hole.
- **`now_slot` is what makes the picture's right edge honest.** Without it, a run of gap slots up to
  the present is invisible — no columns arrive, so nothing says the silence is *current*. It is the
  same information the coverage figure carries for the number.

**4. Rust owns the picture's history; the frontend's canvas is a view that one pull rebuilds.**

A ring of 1200 columns (120 s, the maximum span) at 32 f32 is **153 KB** — the same order as `05`
decision 5's 19 KB and just as unremarkable. The frontend holds no history: `get_spectrogram()`
returns the per-slot columns for the current span, and every event that invalidates the canvas is
the same move, *pull again*, rather than four different repairs.

The rejected alternative was letting the frontend accumulate its own 1200-column ring, which is the
cheapest possible wire contract and has a real symmetry with `05` decision 5 (a window-length change
would re-slice locally). It loses on the dev loop: the picture would be empty for up to two minutes
after every reload, during exactly the ticket that does nothing but reload. Keeping *both* rings was
rejected for the ordinary reason — two copies of one history, two places that can disagree about
what a gap slot was.

**5. Reset does not clear the picture. Only the window and the max hold, as `05` decision 10 said.**

The collision worth recording, because neither `05` nor `07` noticed it: three seconds after a Reset
the number reads `3s of 60s` for the new talk while the picture still shows 57 s of the previous
one's applause.

Settled on **asymmetry of cost**. The number refills honestly and cheaply — `05` decision 1 made a
filling window an ordinary state with coverage climbing — but 60 s of rumble stripe cannot be
recovered once wiped, and the stripe is the entire reason the picture exists. It also keeps a
mis-tapped Reset from getting more expensive, which `05`'s own residual-risk list already flagged.

What it costs is a phrase rather than a property: `07` decision 8 has to be read as *the picture
spans the same seconds as the number*, not *the picture holds the same data*. And `07` decision 7 had
already made that true by making the picture a deliberately different quantity — always unweighted,
"two independent pieces of evidence". Both axes are labelled; neither lies.

**6. The fixed 10 s calibration reading rides in every tick, whether or not that surface is open.**

Two more numbers, ~20 bytes, and 100 more f64 adds against the 600 `05` decision 3 already re-sums
every tick. What it buys is the **absence of mechanism**: no second event, no start/stop commands,
and above all no state in Rust about which view the frontend is showing — a flag that a crash, a
reload or an unmount that never fired could leave set. It also makes the two readings on the
calibration surface impossible to get out of step, since they come out of one re-sum of one ring at
one instant.

**7. Settings ride in every tick *and* are returned by every command. The frontend caches nothing
authoritative.**

The tick is then a complete snapshot of everything the UI paints, which is the strongest available
form of `05` decision 13's ownership rule — and it extends that decision's footgun-denial from
values to **labels**: the unit travels with the numbers, so a value can never be painted under the
wrong one. `06` decision 4's uncalibrated state stops being something the frontend has to remember.

The command return value exists for feel, not for truth: a tap on a picker updates from the
authoritative return rather than waiting up to 100 ms for the next tick. Text entry keeps local
draft state, which is an input buffer and not a second source of truth.

Pulling settings once on mount was rejected because it makes the frontend hold a settings cache that
can be wrong — most sharply after `06` decision 5's silent fallback to defaults on a missing or
corrupt config file, where the live values are not the ones any UI action set. A separate
`settings-changed` event was rejected by fact 6: a startup emit lands before `listen()` exists and is
skipped, so it would need a mount pull *as well*, making it strictly more machinery.

**8. One `input` field crosses: `capturing` | `denied` | `unavailable`. Read from the permission
API.**

A hole no upstream ticket owned. `CLAUDE.md` records that a denied microphone presents as buffers of
**exact zeros** — the stream builds and the callback fires — so under the design as it stood, a
denied mic rendered as `--` beside `0s of 60s` forever, with nothing anywhere saying why, for the
one failure the user can actually fix and only from outside the app.

This does not reopen the no-warnings rule. `11` and `05` refused warnings *about measurements*;
`06` decision 4 set the precedent that an unusual state is shown as a correctly-named state rather
than hidden, and `dBFS` is exactly that. This field fires on a permanent condition requiring action
outside the app, never on a level. `09` renders it as a line of text in the same family as the unit
label — not a banner, not an alert.

Read from `AVAudioApplication`/`AVAudioSession` `recordPermission`, which the spike already calls,
rather than inferred from the exact-zeros signature: an authoritative answer exists, so a heuristic
is the wrong tool. A bare `is_capturing` boolean was rejected for collapsing *fix it in Settings*
and *this is a bug or a missing device* into one message that can only be vague about both — the
shape of message that teaches people to ignore messages.

**9. Hand-written types in one `src/bridge.ts`. Wire names are snake_case on both sides.**

The contract is five types and eight commands, all of which the spec writes down anyway.
`src/bridge.ts` owns every type that crosses plus thin typed wrappers around `invoke` and `listen`,
and is the only file in the frontend that imports `@tauri-apps/api`. No serde rename attributes, so
a Rust field name and its TS field name are **literally the same string** and drift is greppable.

The honest cost, stated because it is a real one: a renamed field is a runtime `undefined`, not a
compile error, and one file plus identical names is the whole mitigation. `ts-rs` would remove that
risk for a dev-dependency and a generated file in the tree — rejected because a stale
`bindings.ts` is worse than a hand-written one, since it looks authoritative. `tauri-specta` would
type command arguments too, which is the most protection available, and is the largest addition of
the three for a five-type contract. Both fail the map's resist-complexity bar rather than any
technical test; if the contract ever grows, `ts-rs` is the one to reach for.

### What makes the frontend re-pull

Short, and one row is a surprise:

| event | re-pull `get_spectrogram()`? |
|---|---|
| mount / webview reload | ✓ |
| canvas resize, orientation change | ✓ |
| window length change (10/30/60/120 s) | ✓ |
| calibration offset change | **—** |
| Reset button | — (decision 5) |
| weighting change (C/A/Z) | — (the picture is unweighted, `07` decision 7) |

The offset row is arithmetic, not an oversight. `07` decision 9 shifts the dB colour window **by
the offset**, and the band values shift with it, so every colour is unchanged — only the legend
relabels. Calibrating is therefore the one settings act that changes every number on screen and no
pixel of the picture.

### Recorded, not debated — forced upstream, or too cheap to argue about

- **`get_spectrogram` is the only bulk payload:** ≈138 KB at a 60 s span, ≈276 KB at 120 s, over the
  custom protocol (fact 7), once per redraw-from-scratch. The live tick stays ~17× under the
  8192-byte fast-path threshold, so it never leaves the fast path.
- **The frontend aggregates columns into the pixel budget, in energy** (`07` decision 6). It is the
  only side that knows the canvas width, and one aggregator beats two — the alternative was passing
  a pixel count into Rust, which would make a canvas resize a command and duplicate the aggregation
  for the live path.
- **Wire dB values are rounded to 0.1 dB**, which is the display resolution `05` decision 12 and
  `09` work at, and it bounds the payload. The rings keep full f64.
- **`Option<f64>` serializes to `null`.** `--` is `null`, never a sentinel like `-999`.
- **The tick is a timer, not audio-driven.** It publishes with no audio at all, which is what makes
  decisions 7 and 8 safe and removes any need for a `get_settings` command — the first tick is
  ≤100 ms after the listener registers.
- **No throttling or coalescing in Rust.** Fact 5 says delivery is ordered and buffered; decision 3
  means a late tick loses nothing.
- **One typed command per setting**, not a generic `set_setting(key, value)` — `06` requires a
  bounds check on the reference value, which is per-command work anyway.
- **No clear-calibration command.** Uncalibrated is the initial state; a wrong offset is retyped.
  Not asked for, and `06` decision 2 already makes the offset directly editable.
- **A config-file write failure is logged, not surfaced**, consistent with `04` decision 5 and `06`
  decision 5. The setting is applied in memory regardless; refusing it would be worse.
- **The idle timer does not cross the bridge.** `11` requires it disabled; Rust sets it once at
  startup. Free finding: per `CLAUDE.md`, Tauri's hardcoded iOS framework list already covers UIKit,
  so `objc2-ui-kit` needs no `bundle.iOS.frameworks` entry and therefore **no `tauri ios init`
  regeneration** — unlike `02`'s AVFAudio/AudioToolbox/CoreAudio.
- **`run_capture_spike` is deleted with the spike.** It is the only command on the bridge today and
  none of it survives.

### Residual risks, stated plainly

- **Hand-written types drift silently.** Decision 9's accepted cost: a renamed field is a runtime
  `undefined` rather than a build failure, and `npm run build`'s `vue-tsc` cannot see across the
  bridge. If the contract grows past what one file makes obvious, `ts-rs` is the escape.
- **The snapshot is untested on a phone.** 276 KB of JSON at a 120 s span parses in a few ms on a
  desk; nobody has run it in a WKWebView on an iPhone. The fallback if it stutters is to pass a
  pixel budget into `get_spectrogram` and aggregate in Rust — which costs the single-aggregator
  property above, so it is a trade rather than a fix.
- **`"denied"` may read differently on macOS.** `CLAUDE.md` records that `tauri dev` gets microphone
  access with no `Info.plist` because TCC attributes the request to the responsible parent process,
  so the desktop dev loop is the permissive case and may report `capturing` where iOS reports
  `denied`. Untested, and it means decision 8's most useful state is the one hardest to exercise on
  the desk.
- **`"unavailable"` conflates causes** — stream build failure, no input device, `01`'s
  category-never-set signature. Detail lives only in the log, which on a device means Xcode.
- **Settings in every tick makes the UI unable to show a mode that is not running**, which is the
  point, but it also means the picker feedback path is the command return value. If `09` finds the
  controls feel sticky, that is where to look, and it is already in the design.

### What this ticket hands downstream

- **Ticket [`09`](09-screen-layout.md) — one new thing to design, and two obligations removed.** New:
  the **input-state line** (decision 8), which needs to read as a state label in the family of
  `dBFS` rather than as a warning, and to say something actionable for `denied`. Removed: the layout
  holds **no history and no settings cache** — it renders the last tick and calls
  `get_spectrogram()` on mount, on resize/orientation change, and on a window-length change, and
  notably **not** after calibration, where only the legend relabels. Also: picker taps get their
  feedback from the command return value, not from the next tick.
- **Ticket [`10`](10-write-the-spec.md) — the contract verbatim, plus three things to state.** The
  wire contract and command list above; that **Reset does not clear the spectrogram** (decision 5)
  with the reason, since it will otherwise read as a bug; and that the **input state is the one
  place the app speaks up**, with the distinction that keeps it inside the no-warnings rule — it
  fires on a permanent condition fixable only outside the app, never on a measurement. Worth
  reproducing the re-pull table too; the offset row is counter-intuitive enough to be worth a line.
- **The map** — nothing new opens. The frontend-testing question stays open and gets *narrower*:
  `src/bridge.ts` is now the only untested seam that matters, and decision 9 accepts that
  deliberately rather than leaving it unexamined.

### Corrections to earlier tickets

- **Ticket `07` decision 8's phrasing.** "The picture is literally what is inside the number" must be
  read as *the same span*, not *the same data* — see decision 5. `07` decision 7 had already made
  the two different quantities, so this is a wording correction rather than a change of substance.
- **Ticket `05`'s reset/clear table gains a fourth column, and it is empty.** Nothing clears the
  spectrogram ring: not Reset (decision 5), not a weighting change (the picture is unweighted), not
  F/S, not a window-length change (a re-slice), not an offset change, and a rate change produces gap
  slots by construction exactly as it does for the energy ring. Worth stating in the spec precisely
  *because* it is all dashes.

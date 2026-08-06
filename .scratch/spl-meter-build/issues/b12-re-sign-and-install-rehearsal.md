# Re-sign and install rehearsal

Parent: [SPL Meter Build](../map.md)
Type: task
Status: resolved
Blocked by: [`b08`](b08-tier-1-device-pass.md)

## The work

Do the day before the talk — around **2026-08-11**. Nothing to decide; this exists because
[the venue run](b13-the-venue-run.md) is blocked until it is done, and because the failure it
guards against is silent until the worst possible moment.

**There is no Apple Developer account.** Free provisioning signs for **7 days** and then the
app refuses to launch. A build made when this map was charted expires **on or about the day of
the talk** — and per spec §15 the expiry **has never actually been hit**, so the re-signing
workflow is unrecorded. That is exactly the sort of thing that takes twenty minutes on a
Tuesday afternoon and is unrecoverable in a venue foyer.

Checklist:

1. Fresh build and install, exactly the loop in spec §2.2:
   ```bash
   npm run build
   env -u FORCE_COLOR npx tauri ios build --debug
   xcrun devicectl device install app --device <udid> \
     src-tauri/gen/apple/build/arm64/decibel-meter.ipa
   xcrun devicectl device process launch --device <udid> net.thezic.decibel-meter
   ```
   `echo $FORCE_COLOR` first — it must be unset.
2. `./scripts/check-ios-plist.sh` — assert `NSMicrophoneUsageDescription` survived the merge.
   Its absence is a launch-time process kill, not a build error.
3. **Check whether the calibration offset survived the install.** Installing over the top
   usually preserves the data container; **delete-then-install does not**, nor does
   regenerating the Xcode project (§13.10).
4. **Write the offset down**, on paper or in a note. If the container is lost at the venue,
   recovery is a retype; without a written copy it is being uncalibrated until you are next
   standing beside the proper meter.
5. Charge the phone, and confirm the app still runs with the screen left on for longer than the
   auto-lock interval.
6. Take the reference meter's details with you if there is any doubt about which one will be
   there — §8.4 step 1 wants the app's weighting to match whatever it shows.

## Done when

The app is freshly signed, installed, launching and capturing on the phone, with the offset
either preserved or written down, and the re-signing workflow **recorded in the resolution** —
that is the fog this ticket clears, and it is worth more than the install itself.

## Resolution

**Rehearsed on 2026-08-06, five days early and deliberately so** — a rehearsal is worth having
*before* the day it protects. The fresh install for the talk still wants repeating the day before;
what does not need repeating is the discovery below, which is the whole reason this ticket exists.

**Finding 1 — a rebuild does not re-sign, and the expiry is measured from the profile's creation,
not from the build.** The `.ipa` built today embedded a profile **created 2026-08-05 and expiring
2026-08-12**: `tauri ios build` happily reused yesterday's cached profile, six days from death, and
said nothing. This is the silent failure the ticket was cut for, arriving in its most plausible
disguise — *I built it this morning, so it must be good for a week.* It is not. **The number that
matters is `ExpirationDate` in the embedded profile, and it must be read, not assumed.**

```bash
unzip -p src-tauri/gen/apple/build/arm64/decibel-meter.ipa \
  "Payload/decibel-meter.app/embedded.mobileprovision" > /tmp/prov.plist
security cms -D -i /tmp/prov.plist | plutil -p - | grep -E "CreationDate|ExpirationDate"
```

**Finding 2 — the re-sign workflow, proven end to end, and it takes 48 seconds.** Deleting the
cached profile is what forces a new one; Xcode's automatic signing then issues it during the build
with no prompt and no interactive login, because the Apple ID session is already valid.

```bash
# 1. the cached profiles live here, one file per profile — identify by application-identifier
ls ~/Library/Developer/Xcode/UserData/Provisioning\ Profiles/
security cms -D -i "…/<uuid>.mobileprovision" | plutil -p - | grep -E "application-identifier|ExpirationDate"

# 2. move the decibel-meter one aside (a backup, not a delete — restore it if renewal fails)
mv "…/9ff2ce33-….mobileprovision" ~/somewhere-safe/

# 3. rebuild: a *new* profile is issued and cached automatically
npm run build
env -u FORCE_COLOR npx tauri ios build --debug

# 4. read back what was actually embedded — finding 1 says do not assume
```

Measured: **new profile `adf74102-…` created 2026-08-06 15:25:17 UTC, expiring 2026-08-13 15:25:17
UTC, `TimeToLive` 7.** `npm run build` + `tauri ios build --debug` including the Apple round trip
was **48 s**; install ~10 s; launch ~2 s. **The whole loop is about a minute**, which is the other
useful thing to know the day before: this is not a job that needs an evening.

**Finding 3 — the 7 days are the *profile's*, and the certificate is not the constraint.** The
signing certificate (`Apple Development: simondhlbrg@gmail.com (9LX7QXMJHK)`, team `QV7UX8JMYX`)
runs to **2027-01-29**. Only the free-provisioning profile expires weekly, so re-signing is a
profile refresh and never a certificate problem — worth knowing, because "signing has expired"
otherwise sounds like it might need an Apple Developer account to fix. It does not.

**Finding 4 — the stored offset can be read off the phone in one command, which is strictly better
than the paper note step 4 asks for.** Checklist step 3 is answered and answered twice: the offset
survived a plain install-over-the-top **and** the re-signed install, same data container
(`331C230B-…`) throughout.

```bash
xcrun devicectl device copy from --device <udid> \
  --domain-type appDataContainer --domain-identifier net.thezic.decibel-meter \
  --source "Library/Application Support" --destination ./backup
```

**The value, written down as step 4 demands: `offset_db: 113.26162153261384`** (with
`weighting: C`, `time_weighting: S`, `window_s: 10` as found). Note `--source .` fails —
`.com.apple.mobile_container_manager.metadata.plist` cannot be read — so name a subpath.

**Finding 5 — and it restores, so a lost container is a one-command repair rather than a retype.**
`copy to` writes into the container and the app reads it at the next launch. Proven rather than
assumed, by round-tripping a harmless field: writing `window_s: 60` and relaunching produced
`settings: Settings { …, window_s: 60, offset_db: Some(113.26162153261384) }` in the device log,
then the original `window_s: 10` was written back and verified the same way. **The phone ends this
ticket in exactly the state it started.** This retires most of §13.10's sting: a free-provisioning
reinstall can still lose the container, but recovery is now a file copy.

```bash
xcrun devicectl device copy to --device <udid> \
  --domain-type appDataContainer --domain-identifier net.thezic.decibel-meter \
  --source ./backup/net.thezic.decibel-meter/settings.json \
  --destination "Library/Application Support/net.thezic.decibel-meter/settings.json"
```

**Installed, launched and capturing on the re-signed build**, with the session exactly as §3.1 asks:
`sample_rate: 48000.0, input_channels: 1, mode: "AVAudioSessionModeMeasurement",
measurement_mode: true, io_buffer_duration: 0.0213, permission_granted: true`, and
`./scripts/check-ios-plist.sh` green — `NSMicrophoneUsageDescription` survived the merge.

**Seen in passing, and it is `b07`/`b08` behaving correctly rather than a defect.** When the phone
locked during the install, the log showed the whole documented chain in nine seconds: a route change
invalidating the stream, the supervisor rebuilding, `setActive` refused with **561015905** (`'!pla'`,
the backgrounded case `b08` measured at 70 s), and `session: reports 0 input channels`. Then it
recovered on returning to the foreground. Nothing to fix; it is the first time that sequence has
been *observed* rather than provoked.

**Left for Simon, because they are physical:** charging the phone; confirming the screen stays on
past the auto-lock interval with the app foreground and untouched (`lib.rs`'s
`setIdleTimerDisabled(true)` is in place and `b01` verified it, but today's log cannot distinguish
"idle timer disabled" from "nobody was watching the phone"); and taking the reference meter's
details for §8.4 step 1.

**One thing to carry into [the venue run](b13-the-venue-run.md): the app now on the phone dies on
2026-08-13 15:25 UTC.** If the talk is on or after that date, re-sign again using finding 2 — and
whenever it is done, re-sign rather than merely rebuild.

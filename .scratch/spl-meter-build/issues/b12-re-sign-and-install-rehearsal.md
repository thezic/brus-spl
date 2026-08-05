# Re-sign and install rehearsal

Parent: [SPL Meter Build](../map.md)
Type: task
Status: open
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

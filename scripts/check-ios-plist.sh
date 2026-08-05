#!/usr/bin/env bash
# Asserts that NSMicrophoneUsageDescription survived the iOS Info.plist merge.
#
# Why this exists (research 01, open risk 10): the merge that copies
# src-tauri/Info.plist into the generated Xcode project is undocumented, its
# precedence order changed silently between tauri-cli 2.4 and 2.9, and the
# upstream issue is still open with no maintainer reply. A missing purpose string
# is not a build failure — it is a process kill on first capture attempt. So it
# deserves a real assertion rather than trust.
#
# Run after `npm run tauri ios build` (or `ios dev`), which is when the merge
# happens. `ios init` does not merge.
set -euo pipefail

cd "$(dirname "$0")/.."

KEY="NSMicrophoneUsageDescription"
found=0
failed=0

while IFS= read -r plist; do
	found=$((found + 1))
	if /usr/libexec/PlistBuddy -c "Print :$KEY" "$plist" >/dev/null 2>&1; then
		echo "ok       $plist"
	else
		echo "MISSING  $plist"
		failed=$((failed + 1))
	fi
done < <(find src-tauri/gen/apple -name "Info.plist" -not -path "*/build/*" 2>/dev/null)

if [ "$found" -eq 0 ]; then
	echo "No generated iOS Info.plist found under src-tauri/gen/apple."
	echo "Run 'npm run tauri ios init' then 'npm run tauri ios build' first."
	exit 2
fi

if [ "$failed" -ne 0 ]; then
	echo
	# `ios init` generates the project but does NOT merge the plist — only
	# dev/build/run do. Distinguish "you haven't built yet" from "the merge broke",
	# because the first is expected and the second is a launch-time process kill.
	# `pod install` runs as part of dev/build, so Pods/ is a decent proxy for
	# "a build has happened at least once".
	if [ ! -d src-tauri/gen/apple/Pods ]; then
		echo "$failed of $found generated plist(s) lack $KEY — but no iOS build"
		echo "has run yet (no src-tauri/gen/apple/Pods). This is expected after"
		echo "'tauri ios init', which generates the project without merging the"
		echo "plist. Run 'npm run tauri ios dev' or 'ios build', then re-run this."
		exit 3
	fi
	echo "$failed of $found generated plist(s) lack $KEY."
	echo "The app will be killed on first microphone access. A build HAS run, so"
	echo "the merge itself is suspect: check that src-tauri/Info.plist exists and"
	echo "that this tauri-cli version still honours it (see research 01 risk 10)."
	exit 1
fi

echo
echo "$KEY present in all $found generated plist(s)."

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
	echo "$failed of $found generated plist(s) lack $KEY."
	echo
	# Deliberately does NOT guess which of these it is. An earlier version tried to
	# infer "has a build run?" from the presence of gen/apple/Pods, which is wrong:
	# this project has no pod dependencies, so Pods/ never appears even after a
	# successful build. That guess failed in the dangerous direction — it reported a
	# genuinely broken merge as the benign "you haven't built yet" case.
	echo "If you have NOT yet run 'tauri ios dev' or 'ios build': expected."
	echo "'ios init' generates the project without merging the plist. Build, re-run."
	echo
	echo "If you HAVE built: this is a real problem. The app is killed on first"
	echo "microphone access, with no build error. Check that src-tauri/Info.plist"
	echo "exists and that this tauri-cli version still honours the merge order"
	echo "(undocumented, and it changed between 2.4 and 2.9 — research 01 risk 10)."
	exit 1
fi

echo
echo "$KEY present in all $found generated plist(s)."

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
	echo "The app will be killed on first microphone access. Check that"
	echo "src-tauri/Info.plist exists and that tauri-cli's merge still honours it."
	exit 1
fi

echo
echo "$KEY present in all $found generated plist(s)."

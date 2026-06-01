#!/usr/bin/env bash
set -euo pipefail

TITLE="${1:-Ristux}"
BOUNDS="${2:-80,80,1360,820}"
IFS=',' read -r LEFT TOP RIGHT BOTTOM <<< "$BOUNDS"

case "$LEFT$TOP$RIGHT$BOTTOM" in
  (*[!0-9-]*|"") exit 0 ;;
esac

for _ in $(seq 1 80); do
  if osascript >/dev/null 2>&1 <<OSA
set targetTitle to "$TITLE"
set leftEdge to $LEFT
set topEdge to $TOP
set rightEdge to $RIGHT
set bottomEdge to $BOTTOM

tell application "System Events"
  repeat with processName in {"qemu-system-x86_64", "QEMU"}
    if exists process processName then
      tell process processName
        repeat with candidateWindow in windows
          if targetTitle is "" or (name of candidateWindow contains targetTitle) then
            set bounds of candidateWindow to {leftEdge, topEdge, rightEdge, bottomEdge}
            return
          end if
        end repeat
        if exists window 1 then
          set bounds of window 1 to {leftEdge, topEdge, rightEdge, bottomEdge}
          return
        end if
      end tell
    end if
  end repeat
end tell
error "QEMU window not ready"
OSA
  then
    exit 0
  fi
  sleep 0.1
done

echo "warning: could not resize QEMU window; resize it manually or grant Accessibility permission to your terminal." >&2

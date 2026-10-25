#!/usr/bin/env bash
# Fails if the opening lines of the plain-language docs contain technical jargon.
# Checks the first N lines of each file, or everything above the line
# "<!-- technical-below -->" when that marker appears earlier.
# A line that ends with "<!-- jargon-ok -->" is skipped (for the product's own
# button names, such as "Copy connection link", until pairing without links ships).
# Usage: scripts/check-jargon.sh   (run from anywhere; paths are relative to the repo root)
# Only "set -u", not "set -e": grep exits non-zero when it finds no match, which
# is the normal case here, and the script collects all findings in "status"
# instead of stopping at the first one.
set -u
cd "$(dirname "$0")/.." || exit 1

# file:number-of-leading-lines-to-check:strict (strict=1 also bans the words in STRICT_WORDS)
TARGETS=(
  "docs/GETTING_STARTED.md:40:1"
  "docs/HELP.md:40:0"
  "docs/GLOSSARY.md:40:0"
  "packaging/windows/installer-info.txt:12:1"
)

# Matched case-sensitively as whole words (acronyms).
CASE_WORDS='IP|TCP|UDP|TLS|SHA|SHA-256|DNS|LAN|APK|ADB|H\.264|H264|JNI|QUIC|MM_INVITE'
# Matched case-insensitively as whole words.
ANY_WORDS='port|ports|codec|codecs|bitrate|kbit|kbit/s|kbps|fps|GStreamer|listener|listeners|mDNS|Bonjour|session id|session ids|PipeWire|Wayland|X11|runtime|handshake|certificate|fingerprint|latency|socket|daemon|localhost|loopback|backend|protocol|bind|MediaCodec|MediaProjection|sideload|mm-host|mm-viewer|stderr|stdout|link|links|address|addresses|package|packages|portal|driver|drivers'
# Also banned where the operating system's own word is not needed (not in HELP or GLOSSARY, which explain them).
STRICT_WORDS='firewall|VPN'

status=0
tmp=$(mktemp) || exit 1
trap 'rm -f "$tmp"' EXIT
for target in "${TARGETS[@]}"; do
  IFS=: read -r file count strict <<< "$target"
  if [ ! -f "$file" ]; then
    echo "MISSING: $file"
    status=1
    continue
  fi
  # Stop at the technical marker, if it is inside the checked range.
  marker=$(grep -n -m1 -F '<!-- technical-below -->' "$file" | cut -d: -f1 || true)
  if [ -n "$marker" ] && [ "$marker" -le "$count" ]; then
    count=$((marker - 1))
  fi
  # Number the lines first so reported line numbers stay correct; drop exempt lines.
  head -n "$count" "$file" | grep -n '' | grep -v -F '<!-- jargon-ok -->' > "$tmp" || true
  words="$ANY_WORDS"
  [ "$strict" = "1" ] && words="$ANY_WORDS|$STRICT_WORDS"
  while IFS= read -r hit; do
    echo "$file:$hit"
    status=1
  done < <(
    { grep -E -w "$CASE_WORDS" "$tmp" || true
      grep -iE -w "$words" "$tmp" || true; } | sort -t: -k1,1n -u
  )
  rm -f "$tmp"
done

if [ "$status" -eq 0 ]; then
  echo "check-jargon: OK (no banned words in the opening lines)"
else
  echo "check-jargon: FAILED - rewrite the lines above in plain words, or move the detail further down."
fi
exit "$status"

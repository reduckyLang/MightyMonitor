#!/usr/bin/env bash
# Fails if the opening lines of the plain-language docs contain technical jargon.
# Usage: scripts/check-jargon.sh   (run from anywhere; paths are relative to the repo root)
set -u
cd "$(dirname "$0")/.."

# file:number-of-leading-lines-to-check
TARGETS=(
  "docs/GETTING_STARTED.md:40"
  "docs/HELP.md:40"
  "docs/GLOSSARY.md:40"
  "packaging/windows/installer-info.txt:12"
)

# Matched case-sensitively as whole words (acronyms).
CASE_WORDS='IP|TCP|UDP|TLS|SHA|SHA-256|DNS|LAN|APK|ADB|H\.264|H264|JNI|QUIC|MM_INVITE'
# Matched case-insensitively as whole words.
ANY_WORDS='port|ports|codec|codecs|bitrate|kbit|kbit/s|kbps|fps|GStreamer|listener|listeners|mDNS|Bonjour|session id|session ids|PipeWire|Wayland|X11|runtime|handshake|certificate|fingerprint|latency|socket|daemon|localhost|loopback|backend|protocol|bind|MediaCodec|MediaProjection|sideload|mm-host|mm-viewer|stderr|stdout'

status=0
for target in "${TARGETS[@]}"; do
  file="${target%%:*}"
  count="${target##*:}"
  if [ ! -f "$file" ]; then
    echo "MISSING: $file"
    status=1
    continue
  fi
  head -n "$count" "$file" > /tmp/check-jargon.$$ || { status=1; continue; }
  while IFS= read -r hit; do
    echo "$file: $hit"
    status=1
  done < <(
    { grep -nE -w "$CASE_WORDS" /tmp/check-jargon.$$ || true
      grep -niE -w "$ANY_WORDS" /tmp/check-jargon.$$ || true; } | sort -t: -k1,1n -u
  )
  rm -f /tmp/check-jargon.$$
done

if [ "$status" -eq 0 ]; then
  echo "check-jargon: OK (no banned words in the opening lines)"
else
  echo "check-jargon: FAILED - rewrite the lines above in plain words, or move the detail further down."
fi
exit "$status"

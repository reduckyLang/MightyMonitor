#!/usr/bin/env python3
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "android" / "app"
STRINGS = APP / "src" / "main" / "res" / "values" / "strings.xml"
DEBUG_BOOL = APP / "src" / "debug" / "res" / "values" / "bools.xml"
MAIN_BOOL = APP / "src" / "main" / "res" / "values" / "bools.xml"
BANNED = [
    "IP", "port", "TCP", "UDP", "TLS", "SSL", "certificate", "fingerprint", "token",
    "codec", "H.264", "bitrate", "kbit", "fps", "session id", "handshake", "socket",
    "bind", "listener", "mDNS", "RTT", "ABR", "endpoint", "legacy", "unencrypted",
    "invitation", "payload", "protocol", "link", "address", "runtime", "driver",
    "package", "portal", "Wayland", "PipeWire", "GStreamer", "SHA", "firewall", "VPN",
]


def fail(message):
    print(message, file=sys.stderr)
    return 1


def main():
    try:
        root = ET.parse(STRINGS).getroot()
    except (ET.ParseError, OSError) as error:
        return fail(f"Could not read {STRINGS}: {error}")

    offending = []
    for item in root.findall("string"):
        key = item.get("name", "")
        if key.startswith("adv_"):
            continue
        value = "".join(item.itertext())
        for word in BANNED:
            # The product's own paste-a-link and copy-link labels may use "link" (keys prefixed link_).
            if word == "link" and key.startswith("link_"):
                continue
            pattern = re.escape(word) if word == "H.264" else rf"\b{re.escape(word)}\b"
            if re.search(pattern, value, re.IGNORECASE):
                offending.append(key)
                break
    if offending:
        print("Strings containing banned words: " + ", ".join(sorted(offending)), file=sys.stderr)
        return 1

    main_sources = APP / "src" / "main"
    banned_source_phrases = re.compile(rb"Allow older alpha|unencrypted", re.IGNORECASE)
    offending_files = []
    for path in main_sources.rglob("*"):
        # Only hand-written sources count; skip CI-built native libraries in jniLibs.
        if path.is_file() and "jniLibs" not in path.relative_to(main_sources).parts:
            try:
                content = path.read_bytes()
            except OSError as error:
                return fail(f"Could not read {path.relative_to(ROOT)}: {error}")
            if banned_source_phrases.search(content):
                offending_files.append(path.relative_to(ROOT).as_posix())
    if offending_files:
        return fail("Legacy toggle text found in main sources: " + ", ".join(sorted(offending_files)))

    bool_files = list((APP / "src").rglob("*.xml"))
    true_definitions = []
    main_default = None
    for path in bool_files:
        try:
            values = ET.parse(path).getroot()
        except (ET.ParseError, OSError) as error:
            return fail(f"Could not read {path}: {error}")
        for item in values.findall("bool"):
            if item.get("name") == "legacy_toggle":
                value = (item.text or "").strip().lower()
                if value == "true":
                    true_definitions.append(path.resolve())
                if path.resolve() == MAIN_BOOL.resolve():
                    main_default = value
    if true_definitions != [DEBUG_BOOL.resolve()]:
        actual = ", ".join(str(path.relative_to(ROOT)) for path in true_definitions) or "none"
        return fail(f"Only {DEBUG_BOOL.relative_to(ROOT)} may define legacy_toggle as true; found: {actual}")
    if main_default != "false":
        return fail(f"{MAIN_BOOL.relative_to(ROOT)} must define legacy_toggle as false")

    print("Android strings and legacy toggle checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

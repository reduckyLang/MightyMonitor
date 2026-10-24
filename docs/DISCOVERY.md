# Discovery library (M8, part 1)

Discovery never grants trust. LAN peers and their names, IDs and addresses are
untrusted and may be spoofed. Keep pinned TLS and private invitation authorization;
no pairing or persistent identity is implemented here.

Advertising is explicit opt-in through `Advertisement::start`. Dropping its guard
unregisters the service and shuts down the daemon. Browse is on-demand, limited
to 30 seconds and 64 deduplicated services; entries expire after 120 seconds.
The backend trait supports deterministic tests without multicast.

Only `_mightymonitor._tcp.local.`, a sanitized 40-character device name, port,
wire version and caller-supplied 1–16 lowercase hex opaque ID are advertised.
TXT keys are `name`, `id`, `wire`. Never supply a secret as the name or ID.
No link, token, full fingerprint, certificate or key belongs in discovery.
Parsing bounds TXT size and rejects invalid UTF-8, duplicate keys,
invalid names/IDs and unsupported versions; unknown keys are ignored.

The library is not wired into host/viewer or Android. Unit tests provide library
evidence only. The real multicast loopback test is ignored by default. Real-device
Wi-Fi, Windows firewall and Android discovery behavior remain unverified.

Backend boundary: mdns-sd normalizes incoming TXT properties (including keeping
only the first duplicate key) before exposing resolved services. The pure parser
rejects duplicates when supplied the original records, but cannot recover duplicate
wire records discarded by mdns-sd. Discovery remains untrusted regardless.

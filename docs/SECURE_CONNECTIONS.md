# Private viewing links

Desktop Share and Android Share generate a fresh connection link for each sharing
session. Start sharing, copy the link, transfer it directly to your other device,
and paste it into View. A phone camera can scan the desktop QR code and open
the link in the Android app; press Connect to begin viewing. Anyone holding the link can view while that sharing session
is running. Keep it private; stopping sharing closes the connection and invalidates
that session's link. Restarting sharing generates a new identity and token.

Links authorize viewing only. They do not authorize keyboard, mouse, touch, audio,
file access or unattended access. Persistent device pairing is not implemented.

## Transport

The original Trinix protocol runs inside TLS 1.3 using Rustls with the ring
provider. Each sharing session creates a self-signed certificate and a random
256-bit viewing token. The link contains the destination, SHA-256 certificate
fingerprint and token, encoded with unpadded base64url. The receiver validates the
fingerprint before trusting that certificate; Rustls still checks its certificate
chain, hostname and handshake signature. A small bounded certificate preface is
public; viewing tokens and screen data are sent only inside TLS. The server accepts
the protocol only after a constant-time token check. Early data is disabled.

The same implementation supports asynchronous desktop transports and synchronous
Android networking workers. Authentication failure ends the connection instead of
retrying or falling back to plaintext. Handshakes have a total eight-second deadline;
Android stop interrupts stalled handshakes. Complete video writes remain bounded.

## Development compatibility

The command-line host retains legacy plaintext mode for protocol development:
pass `--secure --bind YOUR_LAN_IP:5000` to obtain a secure invitation. The invitation
is printed once on standard output as `MM_INVITE ...`; diagnostic messages go to
standard error. Treat standard output as a secret. The GUI always starts a secure
host. Old alpha viewers cannot connect to it.

Viewing old plaintext hosts requires explicitly enabling the older-alpha option
under Connection settings. A secure link never silently downgrades to that mode.
Do not expose development listeners to the internet. Network discovery and stored
trusted-device identities are separate future features.

## Validation

Tests cover pinned TLS payloads, wrong certificate and token rejection, strict
link parsing, blocking/asynchronous interoperability, terminal reconnect rejection,
stalled-peer cancellation, Android Rust client/server video events, and real desktop
H.264 encoding/transport/decoding through TLS. Android emulator checks exercise
MediaCodec in both directions on Android 10, 11, 14 and 16. See the workflow results
for the current commit; physical-device acceptance is a separate check.

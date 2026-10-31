# Private viewing links

Desktop Share and Android Share generate a fresh connection link for each sharing
session. Start sharing, copy the link, transfer it directly to your other device,
and paste it into View. A phone camera can scan the desktop QR code and open
the link in the Android app; press Connect to begin viewing. Anyone holding the link can view while that sharing session
is running. Keep it private; stopping sharing closes the connection and invalidates
that session's link. Restarting sharing generates a new identity and token.

Links authorize viewing only. They do not authorize keyboard, mouse, touch, audio,
file access or unattended access. Persistent desktop pairing is also available through the security API and CLI, as described below.

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
Do not expose development listeners to the internet. Discovery wiring, desktop pairing UI and Android pairing remain future work.

## Validation

Tests cover pinned TLS payloads, wrong certificate and token rejection, strict
link parsing, blocking/asynchronous interoperability, terminal reconnect rejection,
stalled-peer cancellation, Android Rust client/server video events, and real desktop
H.264 encoding/transport/decoding through TLS. Android emulator checks exercise
MediaCodec in both directions on Android 10, 11, 14 and 16. See the workflow results
for the current commit; physical-device acceptance is a separate check.

## Persistent device pairing

`trinix-host --pair --bind ADDRESS:PORT` explicitly enables one interactive pairing
attempt for up to 60 seconds. `trinix-devices --pair ADDRESS:PORT` (security crate,
`async` feature) starts the other side. Both users compare the six-digit SAS
and confirm locally. Dedicated pairing/viewing prefaces and ALPN (`pair-v2`,
preface `TXDEV2`) prevent confusion with private links and with earlier builds.
SAS comparison is required when exchanging initially untrusted certificates.
Nearby devices, addresses and display names never establish trust.

SAS construction (commit-reveal). Before the TLS handshake each side sends
`SHA-256(label || role || nonce)` for a fresh random 256-bit nonce. Inside TLS
each side reveals its nonce; the peer verifies it against the commitment and
aborts terminally on a mismatch, before any confirmation prompt. The code is the
first block of HKDF-SHA256 (salt = viewer nonce || host nonce, input = TLS 1.3
exporter material, fixed info string), reduced to six decimal digits. Because
both nonces are fixed by commitments before either handshake completes and are
revealed only afterwards, an intermediary that terminates the two TLS legs cannot
search handshake values (for example server randoms) offline for a pair of legs
that display the same code: each honest side contributes a nonce the intermediary
cannot know when it chooses its own handshake values. Its remaining chance is a
single guess of about one in a million per attempt, and attempts are serialized,
spaced and time-limited by the host. This protects only the exchange of initially
untrusted certificates when both users really compare the code. A six-digit code is
short; pair on a trusted network, do not confirm a code you did not compare, and
unpair any device you do not recognize. The wire format is not compatible with
earlier pairing builds; there is no downgrade path.

The persistent self-signed certificate/private key is generated once. Configuration
lives in `XDG_CONFIG_HOME/trinix`, otherwise `~/.config/trinix` on
Unix, and `%LOCALAPPDATA%\trinix` on Windows. `identity.bin` holds the identity,
`name.bin` its friendly name and `peers.bin` the versioned paired-device store.
Private files use atomic sibling-temporary-file replacement; Unix directories
are 0700 and files 0600. Oversized, corrupt, duplicate or stale store writes fail
closed. An exclusive temporary file serializes writers; a leftover file after a
crash can require local removal after checking no writer is active. The store
is limited to 64 peers and 16 KiB. Secrets use zeroizing storage and redacted
Debug implementations. Local account compromise and credential-file copying are
outside the network threat model; protect the account and configuration backups.
No new third-party packages are introduced.

Each pairing stores a certificate fingerprint and random 256-bit shared secret,
exchanged only inside confirmed TLS. Reconnect pins the stored certificate and
checks the pairing secret in constant time; public trust, TOFU fallback, plaintext
downgrade and retry after authentication failure are unavailable. Paired CLI
sharing is view-only, including when `--allow-control` is supplied. Existing
private-link control still requires host permission and a fresh viewer request.
Denied or timed-out handshakes return no peer to persist. Persistence is a local
operation on each side; transport interruption during final acknowledgement or
local write failure may require forgetting an incomplete pairing and repeating it.

`DeviceStore::forget(device_id)` returns whether a peer existed and persists its
removal; `forget_all()` persists an empty store. These APIs work on both devices.
`trinix-host --unpair ID`, `trinix-devices --unpair ID` and the helper's `--forget-all`
provide local maintenance. Forget on each device to remove both local records;
revocation is not an unauthenticated remote erase operation. Host authorization
reopens the store on every new paired connection and Resume. An already streaming
view is not automatically disconnected by an external store edit; stop sharing
to close it immediately. Reconnect cannot restore a forgotten credential.

## UI-independent pairing hooks

`DeviceIdentity::friendly_name()` and `set_friendly_name()` expose/persist a
sanitized display label (at most 32 Unicode characters). The default derives from
the OS hostname where available, with a neutral fallback. IP-like labels and
host:port text become `Device`; names never authorize a connection.
`DeviceStore::list()` returns only a short device ID, friendly name, paired-at
Unix timestamp and last-seen Unix timestamp, without routes or credentials.
Host paired authentication updates last-seen; this is not presence detection.

SAS confirmation and host-shown code entry are distinct modes. The network
handshake implements SAS confirmation on both devices. `PairingCode` and
`PairingAttemptLimiter` are reusable hooks for later code-entry wiring, with
unbiased six-digit generation, constant-time comparison, a 60-second lifetime,
one successful use and lockout after three wrong entries. Keep one host-owned
limiter across reconnects. Correct input cannot clear lockout. Only an explicit
local restart after a 60-second cooldown creates a new attempt. `Clock` is
injectable for deterministic expiry/cooldown tests. Code entry alone does not
replace pinned TLS, SAS or local confirmation in the current network API.

`PairingMode::create_offer()` produces a versioned `PairingOffer` encoded as a
compact `trinix-pair://` base64url string. It carries a random 128-bit
nonce, host certificate fingerprint, address hint and expiry (at most 60 seconds
for the enabled mode); it contains no viewing token, long-lived pairing secret
or private key. Parsing is bounded to 256 characters, validates expiry through
an injectable clock and returns one generic terminal error. Address hints stay
inside the intentionally transferred offer; display models expose no addresses.
`pair_client_offer()` pins the offered certificate before confirmation. The host
atomically consumes the nonce inside TLS before SAS confirmation: denial or
failure after presentation cannot reuse it, and concurrent double-use has one
winner. `consume_offer()` provides the same single-use operation for service
adapters. A presented nonce is compared before the offer is taken, so a wrong
nonce is rejected without consuming the genuine offer; an expired offer is dropped.
Presenting the right nonce consumes it even if confirmation then fails. Replacing or
dropping its `PairingMode` revokes it; expiry also rejects it. A connection that
fails before presenting a nonce cannot consume an offer it has not presented.
Offers still require explicit local host confirmation; there is no setting to
bypass it. QR rendering, code-entry transport, discovery and GUI are not included.

## Authenticated desktop Resume

Desktop TLS accepts Resume only for a known random 64-bit logical session ID
bound to the transport-established `AuthenticatedPeer`. A paired principal uses
the pinned fingerprint and credential generation; a private-link principal is
specific to that share's certificate/token. Reauthentication under another share
or another paired device cannot claim the session. Paired revocation is checked
again on Resume, including after TLS authentication. The registry holds at most
16 sessions, expires entries after five minutes without sent video and rejects
`last_seq` beyond the highest sequence reserved for sending. Failed requests
receive `HelloAck { accepted: false, session_id: 0 }` then close without video.
Live video resumes at the server's next sequence with a fresh keyframe; historical
video and input are never replayed. Mid-stream Resume still requests an IDR.
Explicit legacy plaintext sessions retain their unauthenticated compatibility
behavior and must not be treated as authenticated identities. In that mode a
mid-stream Resume carrying the fixed legacy session id is accepted without any
identity or `last_seq` check and only requests a keyframe. The blocking
Android transport remains private-link based; persistent pairing is not wired there.

Attempt-limiting state (the spacing between attempts, the single active attempt
and `PairingAttemptLimiter` lockout) is held in process memory: it applies to the
whole host process rather than per peer, and a restart clears it. A single
unauthenticated peer can therefore occupy or lock the window, which is a
denial-of-service limit rather than a trust bypass.

Loopback automated coverage includes mutual SAS/denial/timeout, commitment/reveal
mismatch in both directions, nonce binding of the code, two-leg interception
showing different codes, unpaired/revoked/wrong-pin/wrong-secret rejection, store
persistence and bounds, name/code/offer hooks, view-only paired video, and Resume
identity/sequence/expiry/eviction checks. Physical pairing, LAN recovery, Windows
runtime and Android pairing are not verified by those tests. This work is not a
release or a physical acceptance result.

## Discovery and identity

Discovery service names, TXT short device IDs and friendly names are spoofable
routing/display hints. Identity is the full 32-byte certificate fingerprint
established through pairing. Paired connections use an address plus the pin from
the local store; offer-based pairing uses the offer pin, never advertised data.
Pin mismatch is terminal, with no fallback. Authentication lookups, revocation and
Resume use the full fingerprint/authenticated principal. Local short-ID selections
(including forget/unpair) resolve to stored fingerprints and reject ambiguity.

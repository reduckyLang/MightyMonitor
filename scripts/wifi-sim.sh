#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: sudo scripts/wifi-sim.sh setup|teardown INTERFACE
Linux only; requires root and iproute2 (tc). Setup adds an egress root netem
qdisc: delay uniformly distributed from 5 to 30 ms, loss 1%. Teardown removes
it. Existing root qdiscs are not replaced by setup. Teardown removes the root
qdisc; use only on a dedicated test interface. Affects all egress traffic,
including remote administration. No automatic cleanup: always run teardown.
USAGE
}

if [[ $# -eq 1 && ( $1 == --help || $1 == -h ) ]]; then
    usage
    exit 0
fi
if [[ $# -ne 2 || ( $1 != setup && $1 != teardown ) ]]; then
    usage >&2
    exit 2
fi
if [[ $(uname -s) != Linux ]]; then
    echo 'wifi-sim requires Linux.' >&2
    exit 1
fi
if [[ $EUID -ne 0 ]]; then
    echo 'wifi-sim requires root (use sudo).' >&2
    exit 1
fi
command -v tc >/dev/null
if [[ $1 == setup ]]; then
    tc qdisc add dev "$2" root netem delay 17500us 12500us loss 1%
else
    tc qdisc del dev "$2" root
fi

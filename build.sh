#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
contract=${1:?Usage: bash build.sh calendar|dao|project [cargo-contract options]}
shift
case "$contract" in
  calendar|dao|project) buffer=294912 ;;
  sandbox-probe) buffer=16384 ;;
  *) echo "Unknown contract: $contract" >&2; exit 2 ;;
esac
# Decode a complete v1 signed payload while retaining Kreivo's 16 KiB value limit.
env INK_STATIC_BUFFER_SIZE="$buffer" cargo +1.88.0 contract build --release --manifest-path "$root/$contract/Cargo.toml" "$@"

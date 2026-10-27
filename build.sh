#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
contract=${1:?Usage: bash build.sh dao|project|sandbox-probe [cargo-contract options]}
shift
case "$contract" in
  dao|project) buffer=294912; manifest="$root/$contract/Cargo.toml" ;;
  sandbox-probe) buffer=16384; manifest="$root/tests/fixtures/$contract/Cargo.toml" ;;
  *) echo "Unknown contract: $contract" >&2; exit 2 ;;
esac
# Decode a complete v1 signed payload while retaining Kreivo's 16 KiB value limit.
env INK_STATIC_BUFFER_SIZE="$buffer" cargo +1.88.0 contract build --release --manifest-path "$manifest" "$@"

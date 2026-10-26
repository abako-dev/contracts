#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
contract=${1:?Usage: bash build-v5.sh calendar-v5|directory-v5|projects-v5 [cargo-contract options]}
shift
case "$contract" in
  calendar-v5|directory-v5|projects-v5) ;;
  *) echo "Unsupported v5 contract: $contract" >&2; exit 2 ;;
esac
# Decode a complete v1 signed payload while retaining Kreivo's 16 KiB value limit.
env INK_STATIC_BUFFER_SIZE=294912 cargo +1.88.0 contract build --release --manifest-path "$root/$contract/Cargo.toml" "$@"

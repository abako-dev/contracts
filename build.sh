#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
contract=${1:?Usage: bash build.sh dao|project|sandbox-probe [cargo-contract options]}
shift
version=$(cargo +1.88.0 contract --version)
case "$version" in
  *" 5.0.3-013305a-"*) ;;
  *) echo "Use cargo-contract revision 013305adc77d6f8b93dfc9cdb404710942dd738c (see README)." >&2; exit 1 ;;
esac
case "$contract" in
  dao|project) buffer=294912; manifest="$root/$contract/Cargo.toml" ;;
  sandbox-probe) buffer=16384; manifest="$root/tests/fixtures/$contract/Cargo.toml" ;;
  *) echo "Unknown contract: $contract" >&2; exit 2 ;;
esac
# cargo-contract 5 cleans its target when checking ink buffer settings.
# Isolate outputs so building one member cannot erase another member’s artifacts.
env CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}/$contract" \
  INK_STATIC_BUFFER_SIZE="$buffer" cargo +1.88.0 contract build --release --manifest-path "$manifest" "$@"

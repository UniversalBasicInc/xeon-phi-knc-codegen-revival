#!/bin/bash
# Build day rung 3: cross-compile the two science crates for the card with cargo-knc.
# Run on the EPYC host as root:  bash /data/knc/build-day-3.sh
set -euo pipefail
mkdir -p /data/knc/logs; exec > >(tee -a /data/knc/logs/build-day-3.log) 2>&1
. "$HOME/.cargo/env"
export PATH="$HOME/.local/share/uv/python/cpython-3.12-linux-x86_64-gnu/bin:$HOME/.cargo/bin:$PATH"
export PHI_SDK_ROOT=/data/knc/sdk/opt/mpss/3.8.6
cd /data/knc/repos/xeon-phi-toolchain
for crate in endomc dosemc; do
  echo "===== $crate ($(date +%H:%M))"
  bin/cargo-knc build --manifest-path /data/knc/workloads/$crate/Cargo.toml --release
  bin=/data/knc/repos/xeon-phi-toolchain/build/cargo/knc-native/release/$crate
  echo "binary: $bin"; file "$bin"
  cp "$bin" /data/knc/workloads/$crate-k1om
done
echo "===== rung 3 done ($(date +%H:%M)); binaries: /data/knc/workloads/{endomc,dosemc}-k1om"

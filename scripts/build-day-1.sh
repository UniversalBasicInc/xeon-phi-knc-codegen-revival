#!/bin/bash
# Build day rung 1 + start of rung 2: rustup, then the toolchain's doctor and host self-test.
# Run on the EPYC host as root:  bash /data/knc/build-day-1.sh
set -euo pipefail
mkdir -p /data/knc/logs; exec > >(tee -a /data/knc/logs/build-day-1.log) 2>&1
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
. "$HOME/.cargo/env"
rustup toolchain install nightly-2026-09-01 --profile minimal --component rust-src
export PATH="$HOME/.local/share/uv/python/cpython-3.12-linux-x86_64-gnu/bin:$HOME/.cargo/bin:$PATH"
export PHI_SDK_ROOT=/data/knc/sdk/opt/mpss/3.8.6
export PHI_JOBS=96
cd /data/knc/repos/xeon-phi-toolchain
echo "===== doctor"
python3.12 scripts/phi.py doctor
echo "===== test-host"
python3.12 scripts/phi.py test-host
echo "===== rung 1 done; log: /data/knc/logs/build-day-1.log"

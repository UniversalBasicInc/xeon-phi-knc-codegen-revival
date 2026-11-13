#!/bin/bash
# Build day rung 2: point the toolchain at the local source trees, build LLVM (~1 h), runtime,
# C++17 library, cargo-knc. Run on the EPYC host as root inside tmux:
#   tmux new -s build 'bash /data/knc/build-day-2.sh'
set -euo pipefail
mkdir -p /data/knc/logs; exec > >(tee -a /data/knc/logs/build-day-2.log) 2>&1
. "$HOME/.cargo/env"
export PATH="$HOME/.local/share/uv/python/cpython-3.12-linux-x86_64-gnu/bin:$HOME/.cargo/bin:$PATH"
export PHI_SDK_ROOT=/data/knc/sdk/opt/mpss/3.8.6
export PHI_JOBS=96
cd /data/knc/repos/xeon-phi-toolchain
step() { echo; echo "===== $* ($(date +%H:%M))"; python3.12 scripts/phi.py "$@"; }
step fetch llvm  --local /data/knc/sources/llvm-project
step fetch llama --local /data/knc/sources/llama.cpp
step fetch rust  --local /data/knc/sources/rust
step build-llvm
step test-codegen
step build-runtime
echo "===== hello-knc ($(date +%H:%M))"; bin/clang-knc -O2 examples/hello.c -o build/hello-knc && file build/hello-knc
step build-cxx
step build-rust
echo "===== rung 2 done ($(date +%H:%M)); log: /data/knc/logs/build-day-2.log"

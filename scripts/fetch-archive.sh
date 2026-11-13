#!/usr/bin/env bash
# Mirror the KNC toolchain to /mnt/backup. ~2.7 GB. Root disk is at 96% — never stage to /.
set -euo pipefail

DEST="${DEST:-/srv/knc-archive}"
MPSS="$DEST/mpss-3.8.6"
REPOS="$DEST/repos"

mkdir -p "$MPSS" "$REPOS"

echo "==> destination: $DEST"
df -h "$DEST" | tail -1

BASE="https://archive.org/download/intel-mpss-3.8.6"
# windows.zip deliberately omitted (335 MB, not needed)
for f in mpss-3.8.6-linux.tar mpss-3.8.6-k1om.tar mpss-src-3.8.6.tar mpss-downloadcache-3.8.6.tar; do
  echo "==> $f"
  wget -c -P "$MPSS" "$BASE/$f"
done

echo "==> repos"
for url in \
  https://github.com/subgeniuskitty/xeon-phi-kernel-module \
  https://github.com/apc-llc/gcc-5.1.1-knc \
; do
  name=$(basename "$url")
  if [ -d "$REPOS/$name" ]; then
    git -C "$REPOS/$name" pull --ff-only || true
  else
    git clone "$url" "$REPOS/$name"
  fi
done

echo "==> done"
du -sh "$DEST"
echo "Now record checksums:  find '$DEST' -maxdepth 3 -type f -name '*.tar' -exec sha256sum {} + > '$DEST/SHA256SUMS'"

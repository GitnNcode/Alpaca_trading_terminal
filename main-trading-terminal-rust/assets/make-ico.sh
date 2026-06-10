#!/usr/bin/env bash
# Regenerate assets/icon.ico — the Windows .exe icon embedded by build.rs.
#
# Pure macOS toolchain, no Homebrew/ImageMagick needed: `sips` resizes the
# project logo to each standard icon size, then a small Python3 stdlib script
# packs them into a multi-resolution PNG-in-ICO container (Windows 10/11 reads
# PNG-encoded icon entries natively). windres copies the bytes verbatim into an
# RT_GROUP_ICON resource, so what we pack here is what ships in the .exe.
#
# Run after the logo changes:
#   ./assets/make-ico.sh [path/to/logo.png]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
src="${1:-$here/../../website/assets/logo.png}"
out="$here/icon.ico"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sizes=(16 24 32 48 64 128 256)
for s in "${sizes[@]}"; do
  sips -s format png -z "$s" "$s" "$src" --out "$tmp/$s.png" >/dev/null
done

python3 - "$tmp" "$out" "${sizes[@]}" <<'PY'
import struct, sys
tmp, out, sizes = sys.argv[1], sys.argv[2], [int(x) for x in sys.argv[3:]]
imgs = [(s, open(f"{tmp}/{s}.png", "rb").read()) for s in sizes]
header = struct.pack("<HHH", 0, 1, len(imgs))   # reserved, type=icon, count
entries, blobs, offset = b"", b"", 6 + 16 * len(imgs)
for s, data in imgs:
    dim = 0 if s >= 256 else s                  # 256 is encoded as 0
    entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
    blobs += data
open(out, "wb").write(header + entries + blobs)
print(f"wrote {out}: {len(imgs)} sizes, {len(header) + len(entries) + len(blobs)} bytes")
PY

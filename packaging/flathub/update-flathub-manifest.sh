#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Points a checkout of the Flathub app repo (flathub/io.github.rranslam.Unswiper) at a released tag.
#   packaging/flathub/update-flathub-manifest.sh 0.6.5 /path/to/flathub-checkout
# It only changes values, never the manifest's structure:
#   - the `tag:` and `commit:` lines of the unswiper git source
#   - cargo-sources.json, regenerated from that tag's Cargo.lock
# and checks that the tag's metainfo has a <release version="X.Y.Z">. Run from the upstream repo root
# with the tag fetched. Used by .github/workflows/flathub_update.yml; also fine to run by hand.
set -euo pipefail
ver="${1:?usage: $0 VERSION FLATHUB_DIR}"; dir="${2:?usage: $0 VERSION FLATHUB_DIR}"
tag="v$ver"; app=io.github.rranslam.Unswiper; manifest="$dir/$app.yml"
[ -f "$manifest" ] || { echo "no $manifest" >&2; exit 1; }
commit="$(git rev-list -n1 "$tag")"

git show "$tag:crates/unswiper-gtk/data/$app.metainfo.xml" | grep -q "<release version=\"$ver\"" \
  || { echo "metainfo at $tag has no <release version=\"$ver\">; add it before releasing" >&2; exit 1; }

python3 - "$manifest" "$tag" "$commit" <<'PY'
import re, sys
path, tag, commit = sys.argv[1:]
s = open(path).read()
# The unswiper source is the only git source pointing at github.com/rranslam/unswiper.
pat = re.compile(r"(url: https://github\.com/rranslam/unswiper(?:\.git)?\n(\s+)tag: )\S+(\n\2commit: )[0-9a-f]{40}")
s, n = pat.subn(lambda m: f"{m.group(1)}{tag}{m.group(3)}{commit}", s)
if n != 1:
    sys.exit(f"expected one rranslam/unswiper git source with url/tag/commit lines, found {n}")
open(path, "w").write(s)
PY

tmp="$(mktemp -d)"; git show "$tag:Cargo.lock" > "$tmp/Cargo.lock"
rev=74697c75b630d7330e77250fc13cb5ea688d9479  # same pin as packaging/flatpak/update-cargo-sources.sh
curl -sSfL -o "$tmp/gen.py" "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$rev/cargo/flatpak-cargo-generator.py"
python3 "$tmp/gen.py" "$tmp/Cargo.lock" -o "$dir/cargo-sources.json"
echo "updated $manifest to $tag ($commit) and regenerated cargo-sources.json"

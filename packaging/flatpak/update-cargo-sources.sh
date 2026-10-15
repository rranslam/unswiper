#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Regenerates cargo-sources.json (offline crate sources for the Flatpak build) from Cargo.lock.
# Run after any Cargo.lock change. Needs python3 with aiohttp and tomlkit
# (Debian/Ubuntu: apt install python3-aiohttp python3-tomlkit).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
rev=74697c75b630d7330e77250fc13cb5ea688d9479  # flatpak/flatpak-builder-tools, pinned
gen="$(mktemp -d)/flatpak-cargo-generator.py"
curl -sSfL -o "$gen" "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$rev/cargo/flatpak-cargo-generator.py"
python3 "$gen" "$here/../../Cargo.lock" -o "$here/cargo-sources.json"
echo "wrote $here/cargo-sources.json"

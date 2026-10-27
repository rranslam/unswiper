#!/bin/bash
# SPDX-License-Identifier: MIT
# Stress run of the Linux app on a synthetic library (docs/stress/README.md).
# Usage: run_probe.sh UNSWIPER_BINARY LIBRARY_FOLDER SUPPORT_FOLDER
# Prints the probe lines (main-loop stalls, milestones) and the wall time.
set -euo pipefail
bin=$1
lib=$2
sup=$3
start=$(date +%s%N)
UNSWIPER_PROBE=1 xvfb-run -a "$bin" --library-dir "$sup" --open "$lib" --open-first-visit \
    --quit-after 1500 2>&1 >/dev/null | grep '^probe:' || true
end=$(date +%s%N)
echo "wall_ms=$(((end - start) / 1000000))"

#!/usr/bin/env bash
set -euo pipefail
profile="$1"
test "$profile" = O0 || test "$profile" = O2
test "$(cat /sys/fs/cgroup/memory.max)" = 2147483648
test "$(cat /sys/fs/cgroup/memory.swap.max)" = 0
g++ --version > /evidence/compiler-version.txt
rpm -q mesa-libGL mesa-libEGL mesa-dri-drivers > /evidence/runtime-versions.txt
g++ -std=c++17 "-$profile" -ffp-contract=off -Wall -Wextra -Werror /probe/minimal_gl.cpp -ldl -o /build/minimal_gl > /evidence/build.log 2>&1
timeout --kill-after=5s 120 /build/minimal_gl /evidence/raw --generate-upstream-input > /evidence/results.json 2> /evidence/probe.stderr
for name in memory.max memory.swap.max memory.peak memory.events cpu.max pids.max; do
    cat "/sys/fs/cgroup/$name" > "/evidence/final-$name"
done

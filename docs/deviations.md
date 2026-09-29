# Deviations from OpenColorIO 2.5.2

Where the port intentionally behaves differently from upstream. Each entry needs the
owner's approval. A deviation must never change an output covered by the byte-exact
definition of done (PLAN.md §3) unless it also has a waiver in `waivers.toml`.

Upstream bugs that don't affect outputs (crashes, data races, deadlocks) are fixed in the
port and listed here.

| Id | Upstream behavior | Port behavior | Affects outputs? | Approved |
|---|---|---|---|---|

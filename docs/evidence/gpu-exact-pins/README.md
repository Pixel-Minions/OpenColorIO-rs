# Evidence for waivers W0014-W0018

These files are the root-cause evidence that `docs/waivers.md`, `docs/gpu-finite-exact-pins.md`
and `waivers.toml` cite. They were copied on 2026-11-02 from the owner's local
`kept/gpu-exact-pins/2026-10-30/`, unchanged and in the same layout, so the paths the reports use
between themselves still hold. The one difference is that git stores LF line endings, so a file
that had CRLF no longer has the SHA-256 the manifest lists. They are a record: nothing in the repository runs or reads them,
and no number here is an expected value for a test.

`PRESERVATION.json` lists all 517 files of the original folder with their SHA-256. Only text comes
here, as the owner's decision says (`OWNER-DECISION.md`): source, text and provenance may be
committed, and pixel buffers stay local. So the scripts that read `work/style_*` captures can't be
rerun from this folder alone.

The reports name paths as they stood at the time. `kept/...`, `debug/work/` and `/tmp` were local,
and `work/` was git-ignored then. `GAMUT-ROOT-CAUSE-STATUS.md` is dated before the debugger's
report: its "still unresolved" lines for the four ACES2 cases are answered by
`ACES2-GPU-FINDINGS.md`.

| Waiver | Root cause | Evidence |
|---|---|---|
| W0014 | `style_aces2_nan_bug`: ill-conditioned input | `aces2-debug/ACES2-GPU-FINDINGS.md` section 1, `aces2-debug/nan_bug/`, `aces2-debug/work/sweep.txt`, `aces2-debug/work/knockout.txt` |
| W0015 | `style_aces2_4000nit_p3_rndtrip`: builtin precision | Section 2, `aces2-debug/native/knockout.py`, `aces2-debug/work/knockout.txt` |
| W0016 | `style_aces2_rec709_rndtrip`: builtin precision | Section 3, the same knock-out |
| W0017 | `style_aces2_rgb_to_jmh_fwd`: builtin precision | Section 4, `aces2-debug/rgb_to_jmh/probe.py`, `aces2-debug/work/rgb_to_jmh.txt`, the same knock-out |
| W0018 | `style_aces_gamutcomp13_inv`: `pow` precision | `GAMUT-ROOT-CAUSE-STATUS.md`, `ci-40956371150/release/power-O2.json` |

`aces2-debug/native/acc_lib.glsl` and `check_acc.py`, with `work/check_acc.txt`, are the correctly
rounded builtin emulations the knock-out uses and their validation.

## CI runs

CI artifacts expire after 30 days, so a run's id stops leading to its data. What stays in this
folder for each run a waiver or report leans on:

| Run | What it showed | What stays here |
|---|---|---|
| 40956371150 | Minimal `pow` shader for W0018 | `ci-40956371150/release/power-O2.json`, the measured words. The run's debug and release profiles at O0 and O2 gave this same file, byte for byte |
| 41017434341 | Upstream's C++ comparator gives the five failures with the same error bits and indices as the port's | `ci-41017434341/verified-summary.json`, the summary of that comparison |
| 41125187384 | All four profiles passed: 264 GPU cases, 59 fixed-function, the five pins, rejection controls | `ci-41125187384/verified-summary.json`, the summary, with no raw logs |
| The landing, PR 88 | Normal CI passed on `be369c75` | `P40-COMPLETION.json` |
| 40950573272 and the other runs in the reports | Captures and controls | Their results as the reports state them. Raw data only in the owner's local folder |

The summaries were written when those artifacts were verified. They can't be checked again from
the repository once the artifacts are gone; the local folder keeps the raw data. Each Linux GPU
test run still recomputes these failures live against the wheel, on the exact renderer
`waivers.toml` records, and any change of result fails.

# P97: complete only the missing original snb cases

Owner instruction,2026-11-02: repair run41386558773 and run only what is missing
for snb. Do not start the full matrix. The final matrix requires a separate owner
confirmation and must run on the exact release/tagged commit.

The original run tested ee5671bc54ee23eb2e5ce4a8a05a86f096fd0bf8. Rocky completed
2219 tests in46 native targets, both unchanged full ICC sweeps, and the external
boundary audit, then failed saving proof.json because its parent did not exist.
P95 repairs that writer. Windows kernel shard1 was cancelled at GitHub's six-hour
limit. Its380 retained passes have checked compiler metadata, native selections,
log hashes and actual one-test pass footers;169 cases remain unproven. All other
Windows kernel/LUT jobs passed. No original passing OCIO case is rerun here.

`scripts/sde_resume.py` validates the original run/repository/SHA, all complete
shards, the cancelled shard's exact native selection and result prefix, and both
LUT selections. It hashes a four-part missing-only plan. Rocky's already-validated
timings are scheduling weights only. The original Windows source stays clean at
ee5671bc; the current controller is read separately. The rebuilt full Cargo alias
must retain the original compiler target metadata and unfiltered native names.
Only the169 unproven names execute, using the original snb prefix, quick tier,
uncached wheel, working directories and comparisons, with a four-hour job budget.

The final native job rejects missing/duplicate partitions, any passed-case rerun,
changed plan/source/CPU/native inventory, emulator errors, timeouts and altered raw
logs. It reconstructs only the original cancelled shard, then invokes the existing
strict shard and complete-profile auditors. It runs both1400-request exact-once
boundary audits in CI, without reexecuting their passing OCIO requests. Both proof
files explicitly say `ReleaseQualification=false`: old passes cannot qualify a
newer source. The two completed Rocky ICC sweeps are not rerun.

The historical Windows snb oracle0xc0000005 remains OPEN and unexplained. No such
failure is recorded in the completed original jobs or retained cancelled-shard
passes. Report whether it recurs in the missing execution; absence alone does not
establish its cause.

Evidence outside target: `kept/continuation/2026-11-02/sde-run/`,
`sde-windows-missing-inventory.json`, `sde-windows-retained-cases.json`,
`sde-rocky-recovered-profile.json`, `sde-resume-plan.json`, and
`p97-resume-controls.log`. The eleven adversarial controls include a full synthetic
protocol reconstruction and missing/duplicate/altered/failed evidence rejection.
They synthesize input names and libtest protocol framing, never OCIO expected
outputs. The standard native controls import them on both platforms.

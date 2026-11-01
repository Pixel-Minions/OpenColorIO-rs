# P74: source provenance for the mounted Rocky checkout

The one authorized focused run41351906906 at129caa24 used only its five test jobs;
the full matrix was skipped. Both Rocky ICC jobs built the native test target, then
failed before any SDE comparison: bare git rev-parse HEAD refused the host-owned
/work directory. No report.json or test execution log was produced. Both full ICC
sweeps and their byte comparisons remain unproven. This was an execution defect in
the new focused runner, not evidence of an ICC output mismatch.

Evidence retained outside target under kept/continuation/2026-11-01:
targeted-job-124983774195.log (generated), targeted-job-124983774126.log (corrupt),
targeted-sde-run.json and the corresponding uploaded compiler-artifact files.

Fix scripts/sde_targeted.py's provenance read using one process-scoped
git -c safe.directory=<resolved checkout> -C <checkout> rev-parse --verify HEAD.
Validate the SHA and read it before the build, so an unreadable revision fails
before expensive setup. No persistent Git configuration or wildcard trust is added.
All test selections, case budgets, inputs and comparisons remain unchanged.

Native controls require that exact scoped trust command and an actual successful
checkout revision read. Normal Windows checks run them; Rocky checks run them in
a fresh container before the ordinary gate's existing trust configuration. This
catches the mounted-checkout failure without an SDE execution. The workflow's
targeted-controls prerequisite also retains these controls.

The authorized run budget is used (1/1). Do not rerun or dispatch anything based
on this fix. A further owner decision is required for another ICC execution.
The three original Windows jobs continue; their actual outcomes remain separate.

The reviewable additional-run proposal is sde.yml with targeted=true and
targeted_scope=icc on this corrected source. That selects only the native controls
and the two Rocky snb ICC jobs; Windows and all full-matrix jobs stay excluded.
The all scope preserves the original five-job focused selection for other owners.

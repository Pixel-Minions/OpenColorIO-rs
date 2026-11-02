# P77: one complete snb profile per platform, plus ICC

Owner authorization after run41351906906: ONE further workflow after P76 is on
main, covering strict boundary audit, both Rocky snb full ICC sweeps and a COMPLETE
snb CPU profile on Windows and Rocky Linux9. No other CPU profile is selected.

Dispatch sde.yml with targeted=true,targeted_scope=owner-snb on the validated main
SHA. The existing original four kernel partitions run for snb only on each platform.
The two API LUT sweeps run separately, also through the inventoried fresh-process
runner, with the original six-hour job budget and bounded five-hour sweep watchdogs.
All other per-case budgets remain two hours. No request or comparison changes.

The complete-profile join checks the actual compiler manifest, all native targets,
case results, raw log hashes, successful zero-ignored footers and real Cargo runner
preflight. Kernel/LUT partitions must be disjoint and cover every native name once.
Both must use the identical current source SHA and snb emulator prefix. The join also
requires P76's strict1400-request boundary union from the seven production cases.
Thus boundary cases are checked within the complete profile without duplicating a
separate focused Windows execution. Retained nhm evidence is independently recovered
by P76. The earlier snb oracle crash remains unexplained and OPEN.

The workflow contains15 bounded jobs: one native controller, eight kernel partitions,
two LUT selections, two complete-profile joins and two full ICC sweeps. A failed or
cancelled selection is incomplete and cannot produce a complete-profile proof. The
ICC jobs preserve every original comparison, including bytes beyond31. Native controls
reject incomplete/duplicate cases, changed filters, CPU/source mismatches, missing
inventories/preflight and changed raw logs. They make no CPU-coverage claim themselves.

Automatic SDE events select no production jobs. A full matrix requires explicit
targeted=false plus sde_release_ready.py's all-library-entries-closed check; the owner
authorizes it only once as final qualification after all library work has landed.
Manual runs have a separate concurrency group so an automatic skipped event cannot
cancel the long authorized run. Repository workflow enablement is left unchanged.

Before dispatch: P76/P77 normal required checks and actual main landing, then reserve
the one-run follow-up budget. Record source SHA, run ID and actual results in the kept
continuation ledger. No repeat or broader matrix is authorized by this card.

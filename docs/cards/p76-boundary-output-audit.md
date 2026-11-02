# P76: boundary evidence framing

Run41351906906 passed the plan audit and all seven200-request boundary parts on
Windows nhm and snb. Both jobs then failed in the post-run boundary_union parser.
The output was not captured: one_test already supplies --nocapture. The raw first
record is on libtest's test-name line, for example:

    test legacy_boundary_edges_match_native_wheel_part0 ... P64 boundary part 0/7 original indices: [...]

The ^P64 anchor missed that framing. Remove only the exact expected test-name
prefix before parsing records; keep the raw evidence unchanged. Require all eight
distinct ordered case results, successful actual libtest footers, matching raw-log
hashes, the original plan audit, each exact200-index selection and comparison count,
and the complete1400-index union exactly once. Missing/duplicate/foreign records,
wrong indices or types, changed counts/hashes and failed/ignored cases fail closed.

Native rejection controls run with the existing targeted controls on both normal
CI platforms. Re-audit the retained raw logs without rerunning tests; the corrected
strict audit recovers all1400 inputs exactly once on both CPUs, while the original
jobs remain FAILED. Evidence is kept/continuation/2026-11-01/recovered-boundary-proof.json
and targeted-artifacts/boundary-{nhm,snb}/. This does not explain the earlier snb
oracle crash, which remains open in P63 as the owner directed.

Owner authorization now permits one further workflow after this correction is on
main: the two full Rocky snb ICC sweeps, boundary audit, and one complete snb CPU
profile on each platform. Provision this separately; no full matrix until all
library work is landed, then run it once as the final qualification.

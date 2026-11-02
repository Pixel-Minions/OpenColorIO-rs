# P93: retain the owner's final qualification boundary

The release-audit instruction of 2026-11-02 requires the final full SDE matrix on
the exact commit that gets tagged, after explicit owner confirmation. Source
accounting closing is not a trigger. The opt-level 0 depth tests run only in that
matrix; neither the current targeted snb run nor normal CI claims their coverage.

Owned files: README.md, docs/cards/p63-sde-followups.md and this card. This records
the instruction without starting a run, creating a tag, changing a filter, moving
waivers or altering the one consumed targeted-run budget. Release-path work, GPU
compile checks and notices remain Claude's. Report each actually merged SHA.

# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Full-matrix dispatch is reserved for the one final qualification."""
import pathlib
import tomllib

root = pathlib.Path(__file__).resolve().parents[1]
rows = tomllib.loads((root / 'upstream-map.toml').read_text(encoding='utf-8'))['file']
remaining = [r['path'] for r in rows if r['status'] not in ['done', 'n/a']]
if remaining:
    raise SystemExit('Full SDE matrix is held until all library work lands:\n' + '\n'.join(remaining))
print('All source entries are closed; explicit final-qualification dispatch may proceed.')

# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Build the existing source-keyed wheel client natively before emulating CPU tests."""
import json
import os
import pathlib
import subprocess
import time

started = time.monotonic()
root = pathlib.Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--no-deps', '--format-version=1'], cwd=root))
venv = pathlib.Path(metadata['target_directory']) / 'oracle-venv'
python = venv / ('Scripts/python.exe' if os.name == 'nt' else 'bin/python')
if not python.is_file():
    raise RuntimeError('Run cargo xtask oracle info natively before the SDE helper build')
# Match the existing oracle's environment; only the build tools receive native_build's
# explicit compiler-discovery variables. No caller values enter OCIO expected outputs.
environment = {key: os.environ[key] for key in ('SYSTEMROOT', 'TEMP', 'TMP', 'TMPDIR') if key in os.environ}
script = '''import hashlib,json
from ocio_oracle.native_build import executable
path, provenance = executable()
assert provenance['wheel_version'] == '2.5.2' and path.is_file()
print(json.dumps(dict(Executable=str(path), Provenance=provenance,
                     ExecutableSHA256=hashlib.sha256(path.read_bytes()).hexdigest())))
'''
output = subprocess.check_output([str(python), '-X', 'utf8', '-c', script],
                                 cwd=root / 'oracle', env=environment, timeout=600)
result = json.loads(output)
if not all(len(result['Provenance'][key]) == 64 for key in ('source_key', 'library_sha256')):
    raise RuntimeError('Incomplete native wheel-client provenance')
result['SetupSeconds'] = round(time.monotonic() - started, 2)
print(json.dumps(result, indent=2))

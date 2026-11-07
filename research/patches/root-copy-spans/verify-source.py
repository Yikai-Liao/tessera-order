# SPDX-License-Identifier: BSD-2-Clause
"""Verify a reconstructed source tree against the packet's per-file hashes."""
import hashlib
import json
import pathlib
import sys

tree = pathlib.Path(sys.argv[1])
manifest = json.loads(pathlib.Path(sys.argv[2]).read_text())
for name, expected in manifest['files'].items():
    actual = hashlib.sha256((tree / name).read_bytes()).hexdigest()
    if actual != expected:
        raise SystemExit(f'Hash mismatch: {name}')
if 'source_identity' in manifest:
    encoding = json.dumps(manifest['files'], sort_keys=True, separators=(',', ':'))
    assert hashlib.sha256(encoding.encode()).hexdigest() == manifest['source_identity']
else:
    encoding = ''.join(f'{value}  {name}\n' for name, value in sorted(manifest['files'].items()))
    assert hashlib.sha256(encoding.encode()).hexdigest() == manifest['identity_sha256']
print(f'Verified {len(manifest["files"])} source files.')

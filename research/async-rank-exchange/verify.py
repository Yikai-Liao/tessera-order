# SPDX-License-Identifier: BSD-2-Clause
"""Verify pinned prerequisite replay and exact archival root/leaf identities only."""
from pathlib import Path
import argparse, hashlib, io, json, subprocess, tarfile, tempfile
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('repository')
a = p.parse_args()
packet = Path(__file__).resolve().parent
m = json.loads((packet / 'source-map.json').read_text())
def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()
for name, digest in m['root_source_sha256'].items():
    assert sha(packet / name) == digest, name
with tempfile.TemporaryDirectory(prefix='async-source-verification-') as tmp:
    root = Path(tmp)
    data = subprocess.check_output(['git', '-C', a.repository, 'archive', m['prerequisite_commit'], m['prerequisite_path']])
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        tar.extractall(root, filter='data')
    export = root / 'replay'
    subprocess.run(['python3', str(root / m['prerequisite_path'] / 'verify.py'), str(Path(a.repository).resolve()), '--export', str(export)], check=True)
    actual = {str(f.relative_to(export)): sha(f) for f in (export / 'leaf/src').rglob('*.rs')}
    assert actual == m['unchanged_leaf_sha256']
    for name in ['broker.rs', 'path_restore.rs', 'probe.rs', 'restore.rs']:
        assert (packet / 'src' / name).read_bytes() == (export / 'src' / name).read_bytes()
print('Pinned prerequisite replay, unchanged leaf and linked modules, and exact root hashes passed. No build, test or timing executed.')

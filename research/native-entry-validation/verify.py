# SPDX-License-Identifier: BSD-2-Clause
"""Replay the published combined base and validation-only patch without building."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('repository', help='Repository containing the public base and published packet')
    parser.add_argument('--output', help='Optional verification report path')
    args = parser.parse_args()
    packet = Path(__file__).resolve().parent
    source = json.loads((packet / 'source-map.json').read_text())
    provenance = json.loads((packet / 'provenance.json').read_text())
    repository = str(Path(args.repository).resolve())
    dependency = subprocess.check_output([
        'git', '-C', repository, 'show',
        provenance['published_dependency_commit'] + ':' +
        provenance['published_dependency_path'] + '/' + provenance['dependency_patch'],
    ])
    assert hashlib.sha256(dependency).hexdigest() == source['dependency_patch01_sha256']
    patch = packet / 'patches/validation-tests.patch'
    assert digest(patch) == source['test_patch_sha256']
    archive = subprocess.check_output([
        'git', '-C', repository, 'archive', source['public_commit'],
        'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md', 'rust/tessera-sort', 'bench/rust',
    ])
    with tempfile.TemporaryDirectory(prefix='native-validation-replay-') as directory:
        root = Path(directory)
        with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
            bundle.extractall(root, filter='data')
        subprocess.run(['git', '-C', directory, 'apply', '-'], input=dependency, check=True)
        actual = {str(p.relative_to(root)): digest(p)
                  for p in (root / 'rust/tessera-sort').rglob('*.rs')}
        assert actual == source['base_combined_rust_map']
        subprocess.run(['git', '-C', directory, 'init', '-q'], check=True)
        subprocess.run(['git', '-C', directory, 'add', '.'], check=True)
        # The index contains exact combined blobs, enabling actual three-way replay.
        subprocess.run(['git', '-C', directory, 'apply', '--3way', '--check', str(patch)],
                       check=True, capture_output=True)
        subprocess.run(['git', '-C', directory, 'apply', '--3way', str(patch)],
                       check=True, capture_output=True)
        actual = {str(p.relative_to(root)): digest(p) for p in root.rglob('*')
                  if p.is_file() and '.git' not in p.relative_to(root).parts}
        assert actual == source['final_source_map']
        for name, checksum in source['unchanged_public_metadata'].items():
            assert digest(root / name) == checksum
        suffix = ('\n#[cfg(test)]\nmod native_borrow_tests;\n\n'
                  '#[cfg(all(test, not(miri)))]\nmod native_sparse_tests;\n')
        engine = root / 'rust/tessera-sort/src/engine/parallel.rs'
        text = engine.read_text()
        assert text.endswith(suffix)
        normalized = text[:-len(suffix)].encode()
        assert hashlib.sha256(normalized).hexdigest() == source['base_combined_rust_map'][
            'rust/tessera-sort/src/engine/parallel.rs']
    report = {'passed': True, 'base_rust_files': len(source['base_combined_rust_map']),
              'final_source_files': len(source['final_source_map']),
              'three_way_patch_replay': True, 'production_equivalent': True,
              'public_metadata_unchanged': True, 'builds_tests_or_timing_run': False}
    text = json.dumps(report, indent=2) + '\n'
    if args.output:
        Path(args.output).write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()

# SPDX-License-Identifier: BSD-2-Clause
"""Replay pinned public source and patches in memory; never build or run sorts."""
import argparse
import hashlib
import json
from pathlib import Path
import posixpath
import re
import subprocess

PACKET = Path(__file__).resolve().parent


def sha(content):
    return hashlib.sha256(content).hexdigest()


def git(repository, *args):
    return subprocess.check_output(['git', '-C', str(repository), *args])


def get_blob(repository, commit, path, expected=None):
    content = git(repository, 'show', commit + ':' + path)
    if expected is not None:
        assert sha(content) == expected, f'published source hash mismatch: {path}'
    return content


def public_files(repository, commit):
    entries = git(repository, 'ls-tree', '-r', '-z', commit, '--',
                  'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md',
                  'rust/tessera-sort', 'bench/rust').split(b'\0')
    modes = {}
    for entry in entries:
        if entry:
            metadata, name = entry.split(b'\t', 1)
            modes[name.decode()] = metadata.split()[0].decode()
    def read(path, seen=()):
        assert path in modes and path not in seen, 'invalid source symlink'
        content = get_blob(repository, commit, path)
        if modes[path] == '120000':
            target = posixpath.normpath(posixpath.join(posixpath.dirname(path), content.decode()))
            assert not target.startswith('../') and not target.startswith('/')
            return read(target, seen + (path,))
        assert modes[path] == '100644'
        return content
    return {name: read(name) for name in modes}


def apply_patch(files, patch):
    """Apply exact unified hunks, rejecting offsets, fuzz and unsupported records."""
    lines = patch.decode().splitlines(keepends=True)
    index = 0
    changed = []
    while index < len(lines):
        if not lines[index].startswith('--- '):
            assert lines[index].startswith(('diff --git ', 'index ', 'new file mode ', 'deleted file mode ')), 'unsupported patch metadata'
            index += 1
            continue
        old_name = lines[index][4:].rstrip('\n')
        assert index + 1 < len(lines) and lines[index+1].startswith('+++ ')
        new_name = lines[index+1][4:].rstrip('\n')
        index += 2
        assert old_name == '/dev/null' or old_name.startswith('a/')
        assert new_name == '/dev/null' or new_name.startswith('b/')
        old_path = None if old_name == '/dev/null' else old_name[2:]
        new_path = None if new_name == '/dev/null' else new_name[2:]
        assert old_path is None or old_path in files
        assert old_path is not None or new_path not in files
        source = files[old_path].decode().splitlines(keepends=True) if old_path else []
        cursor = 0
        output = []
        hunks = 0
        while index < len(lines) and lines[index].startswith('@@ '):
            match = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', lines[index])
            assert match, 'invalid hunk'
            old_start, old_count, new_start, new_count = match.groups()
            old_start, new_start = int(old_start), int(new_start)
            old_count = 1 if old_count is None else int(old_count)
            new_count = 1 if new_count is None else int(new_count)
            position = old_start if old_count == 0 else old_start - 1
            assert cursor <= position <= len(source), 'overlapping hunk'
            output.extend(source[cursor:position])
            cursor = position
            assert len(output) == (new_start if new_count == 0 else new_start-1)
            index += 1
            consumed = produced = 0
            while consumed < old_count or produced < new_count:
                assert index < len(lines)
                line = lines[index]
                index += 1
                assert line[:1] in (' ', '+', '-'), 'unsupported hunk line'
                marker, content = line[0], line[1:]
                if marker in (' ', '-'):
                    assert cursor < len(source) and source[cursor] == content, 'exact hunk context mismatch'
                    cursor += 1
                    consumed += 1
                if marker in (' ', '+'):
                    output.append(content)
                    produced += 1
                assert consumed <= old_count and produced <= new_count
            hunks += 1
        assert hunks, 'file patch without hunks'
        output.extend(source[cursor:])
        if old_path is not None and old_path != new_path:
            del files[old_path]
        if new_path is not None:
            assert not new_path.startswith('/') and '..' not in Path(new_path).parts
            files[new_path] = ''.join(output).encode()
            changed.append(new_path)
        else:
            assert not output
    return changed


def hashes(files):
    return {name: sha(content) for name, content in files.items()}


def fragment(text, start, following):
    begin = text.index(start)
    return text[begin:text.index(following, begin)]


def verify_control(base, region_base, final):
    engine = base['rust/tessera-sort/src/engine.rs'].decode()
    assert final['leaf/src/engine.rs'] == engine.encode() + b'\n// Research-only helper; the original engine above is byte-preserved.\ninclude!("child_mask_entry.rs");\n'
    assert final['leaf/src/lib.rs'] == base['rust/tessera-sort/src/lib.rs'] + b'\n/// Sealed research-only child-entry controls.\n#[doc(hidden)]\npub mod child_mask;\n'
    helper = final['leaf/src/child_mask_entry.rs'].decode()
    entry = fragment(engine, 'fn sort_with_tile_bytes<', '\n/// Tries to finish')
    expected = entry.replace('fn sort_with_tile_bytes<', 'pub(crate) fn sort_root_child<', 1)
    expected = expected.replace('    tile_bytes: usize,\n) {', '    tile_bytes: usize,\n    candidate: Option<u128>,\n) {', 1)
    expected = expected.replace('let varying = block::varying_key_bits(records, extractor);', 'let varying = child_mask_scan(records, candidate, extractor);', 1)
    assert fragment(helper, 'pub(crate) fn sort_root_child<', '\n/// Public scanner') == expected
    scanner = fragment(base['rust/tessera-sort/src/block.rs'].decode(), 'pub(crate) fn varying_key_bits<', '\n/// Branch hint')
    expected = scanner.replace('pub(crate) fn varying_key_bits<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> u128 {', 'fn child_mask_scan_body<T, E: Extract<T>>(\n    records: &[T],\n    candidate: Option<u128>,\n    extractor: &mut E,\n) -> u128 {', 1)
    expected = expected.replace('let full_mask = u128::MAX >> (128 - E::Value::BITS);', 'let full_mask = candidate.unwrap_or(u128::MAX >> (128 - E::Value::BITS));', 1)
    assert fragment(helper, 'fn child_mask_scan_body<', '\n/// Selects diagnostic') == expected
    original_root = region_base['src/lib.rs'].decode()
    root = final['src/lib.rs'].decode()
    for start, following in [('pub fn partition<', '/// Runs a most-significant root digit'), ('pub fn sort<T: Record>', '\n#[cfg(test)]')]:
        assert fragment(root, start, following) == fragment(original_root, start, following)
    assert root.count('unsafe') == original_root.count('unsafe')
    for name in ['leaf/src/child_mask_entry.rs', 'leaf/src/child_mask.rs', 'leaf/src/child_mask/diagnostics.rs']:
        assert 'unsafe ' not in final[name].decode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('repository', help='Repository with the pinned public commits already present')
    parser.add_argument('--output', help='Optional verification report; contains no machine paths')
    parser.add_argument('--export', help='Optional new source-only directory; omitted by default')
    args = parser.parse_args()
    assert __debug__, 'run without Python -O'
    repository = Path(args.repository).resolve()
    manifest = json.loads((PACKET / 'PACKET-MANIFEST.json').read_text())
    for name, expected in manifest['files'].items():
        assert sha((PACKET / name).read_bytes()) == expected, f'packet hash mismatch: {name}'
    provenance = json.loads((PACKET / 'provenance.json').read_text())
    source = json.loads((PACKET / 'source-map.json').read_text())
    native_info = provenance['native_validation']
    native_map = json.loads(get_blob(repository, native_info['commit'], native_info['map_path'], native_info['map_sha256']))
    files = public_files(repository, provenance['public_commit'])
    combined = provenance['combined_patch']
    patch = get_blob(repository, combined['commit'], combined['path'], combined['sha256'])
    apply_patch(files, patch)
    actual_rust = {name: sha(content) for name, content in files.items() if name.startswith('rust/tessera-sort/') and name.endswith('.rs')}
    assert actual_rust == native_map['base_combined_rust_map']
    apply_patch(files, get_blob(repository, native_info['commit'], native_info['patch_path'], native_info['patch_sha256']))
    assert hashes(files) == native_map['final_source_map']
    for name, expected in native_map['unchanged_public_metadata'].items():
        assert sha(files[name]) == expected
    native_base = files.copy()
    for name, expected in source['patches'].items():
        assert sha((PACKET / 'patches' / name).read_bytes()) == expected
    apply_patch(files, (PACKET / 'patches/01-child-entry.patch').read_bytes())
    prefix = 'rust/tessera-sort/src/'
    final = {'leaf/src/' + name[len(prefix):]: content for name, content in files.items() if name.startswith(prefix)}
    root_info = provenance['region_root']
    region = {'src/' + name: get_blob(repository, root_info['commit'], root_info['directory'] + '/' + name, expected)
              for name, expected in root_info['files'].items()}
    region_base = region.copy()
    apply_patch(region, (PACKET / 'patches/02-root-dispatch.patch').read_bytes())
    final.update(region)
    for name in ['correctness.rs', 'fixtures.rs']:
        final['checks/' + name] = (PACKET / 'checks' / name).read_bytes()
    final['make_diagnostic_source.py'] = (PACKET / 'support/make_diagnostic_source.py').read_bytes()
    assert hashes(final) == source['final_replay_files']
    verify_control(native_base, region_base, final)
    if args.export:
        destination = Path(args.export).resolve()
        assert not destination.exists(), 'export must be new; do not overwrite any checkout'
        assert not destination.is_relative_to(repository) and not destination.is_relative_to(PACKET)
        destination.mkdir(parents=True)
        for name, content in final.items():
            target = destination / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    report = {'passed': True, 'combined_rust_files': len(actual_rust),
              'native_validation_source_files': len(native_base), 'control_source_files': len(final),
              'strict_in_memory_patch_replay': True, 'public_metadata_preserved': True,
              'public_entry_bytes_preserved': True, 'same_1025_prefix_scanner': True,
              'root_transport_preserved': True, 'no_new_unsafe': True,
              'source_export_requested': bool(args.export), 'builds_tests_sorts_or_timings_run': False}
    text = json.dumps(report, indent=2, sort_keys=True) + '\n'
    if args.output:
        Path(args.output).write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()

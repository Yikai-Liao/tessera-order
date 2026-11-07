# SPDX-License-Identifier: BSD-2-Clause
"""Materialize a count-only overlay; never modify the timing source or old caches."""
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent

def instrument(text, public):
    if public:
        old = 'let varying = block::varying_key_bits(records, extractor);'
        new = 'let varying = child_mask_count_scan(records, extractor, |records, extractor| {\n        block::varying_key_bits(records, extractor)\n    });'
        assert text.count(old) == 1
        text = text.replace(old, new)
    old = '    let sampled = probe_order(records, extractor);'
    new = old + '''
    crate::child_mask::diagnostics::record_order(match sampled {
        Order::Ascending => "ascending", Order::Descending => "descending",
        Order::Flat => "flat", Order::Unsorted => "unsorted",
    });'''
    assert text.count(old) == 1
    text = text.replace(old, new)
    old = '            if insertion_probe(records, extractor, probe_len, probe) {'
    new = '''            let finished = insertion_probe(records, extractor, probe_len, probe);
            crate::child_mask::diagnostics::record_insertion(probe_len, records.len(), finished);
            if finished {'''
    assert text.count(old) == 1
    text = text.replace(old, new)
    old = '    if E::Value::BITS > 64 {'
    assert text.count(old) == 1
    text = text.replace(old, '    crate::child_mask::diagnostics::record_node();\n' + old)
    return text

def create(destination):
    destination.mkdir()
    source = ROOT / 'leaf/src'
    for path in source.rglob('*'):
        if path.is_file():
            target = destination / path.relative_to(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            if path.name == 'engine.rs':
                text = path.read_text()
                start = text.index('fn sort_with_tile_bytes<')
                end = text.index('\n/// Tries to finish', start)
                target.write_text(text[:start] + instrument(text[start:end], True) + text[end:])
            elif path.name == 'child_mask_entry.rs':
                text = path.read_text()
                start = text.index('pub(crate) fn sort_root_child<')
                end = text.index('\n/// Public scanner', start)
                target.write_text(text[:start] + instrument(text[start:end], False) + text[end:])
            else:
                target.symlink_to(os.path.relpath(path.resolve(), target.parent))

if __name__ == '__main__':
    import sys
    create(Path(sys.argv[1]).resolve())

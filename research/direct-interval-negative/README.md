<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Direct interval probe: rejected research checkpoint

This checkpoint preserves original BSD-2-Clause pure-metadata code, tests, native capture contract, frozen policy/budgets and a negative first-real-input result. It contains no sorting implementation, unsafe code, record contents, pointers, raw graph, full native capture, Python cache, target directory, or machine-specific absolute path.

Read RESULT.md first. The fixed greedy policy exhausts the coefficient-one live-spill cap on the real 10M Row16 P8 native graph after finalizing 99,923 records. Its declared graph representation independently exceeds the coefficient-32 total byte cap. Work stopped there. Neither finding proves a general lower bound for every interval algorithm; a large SCC alone is not an impossibility theorem. No alternate policy, larger budget, second type rescue, record-moving kernel, or sort timing followed.

## Files

- `src/`: executable metadata parser/planner, immutable-TSV adapter, and ten small tests.
- `CAPTURE.md`: exact source allocation/alias contract and external-input schema.
- `DESIGN.md`: original mechanism and safety/space gates, with the rejection update.
- `RESULT.md`: numerical rejection, validation scope, capacity ledger and bounded root-R16 duplication audit.
- `evidence/summary.json`: compact result without raw edges, events or full frontier arrays.
- `evidence/*.log`: both test windows and the full metadata-run result; the first auxiliary stat failure remains visible.
- `evidence/native-capacity-ledger.tsv`: later unchanged native capacity ingredients, not an allocator peak.
- `manifest.json`: every included source/evidence hash, exact external input/result identities, budget-version chronology and omissions.

References in DESIGN/RESULT to sibling research directories identify external evidence, not files bundled here. The large raw result and native capture remain separately retained under their verified hashes. Python diagnostic memory is never claimed to be the proposed Rust workspace.

## Safe replay

Use Python 3 with standard library, bash, GNU timeout and flock. No network, Cargo build, installation, records, or benchmark is needed. Supply the **existing common CPU lock**, never create or replace one to evade other work:

    TESSERA_CPU_LOCK=/path/to/existing/cpu-window.lock bash replay.sh

This runs only the ten small tests under shared FD9 and a 60-second timeout. It prints acquisition, lock identity and release. With the exact separately retained native TSV, the optional form is:

    TESSERA_CPU_LOCK=/path/to/existing/cpu-window.lock bash replay.sh capture.tsv result.json

The script checks the external capture SHA before the metadata-only run. The result path must not already exist. Full replay can produce a large diagnostic JSON, deliberately excluded from this small checkpoint. It is still not a sorting/performance experiment. Under the frozen policy the expected result is `accepted=false`, `rejection=frozen_live_spill_capacity` and an independent total-space rejection. A process exit of zero means the probe ran and reported that negative result, not that the candidate passed.

Do not retune caps or ordering after this result. Any later materially different question needs a separately named, predeclared experiment.

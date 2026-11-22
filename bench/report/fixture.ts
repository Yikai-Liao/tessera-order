// SPDX-License-Identifier: BSD-2-Clause

import { caseGroup, catalog, columnId, SORT } from "../catalog.ts";
import {
  caseId,
  type Dataset,
  distributions,
  lengths,
  type Manifest,
  type Sample,
  TESSERA,
  TESSERA_AA,
  types,
} from "./model.ts";

/** Builds a synthetic release run with Tessera between faster and slower competitors. */
export function createFixture(blocks = 9): Dataset {
  const implementations = [
    { name: "tessera", stable: true, adapter: "common" },
    { name: "std-stable", stable: true, adapter: "common" },
    { name: "radsort", stable: true, adapter: "rust" },
    { name: "glidesort-sqrt", stable: true, adapter: "rust" },
    { name: "voracious", stable: true, adapter: "rust" },
    { name: "std-unstable", stable: false, adapter: "rust" },
    { name: "vqsort", stable: false, adapter: "native" },
  ];
  const columns = implementations.map(({ name, stable, adapter }) => ({
    id: columnId({ implementation: name, execution: "serial" }),
    implementation: name,
    execution: "serial" as const,
    stable,
    adapter,
  }));
  const cases = types.flatMap((ty) =>
    distributions.flatMap((distribution) =>
      lengths
        .filter((n) => n <= 1000000 || !["record128", "owned"].includes(ty))
        .map((len) => {
          const measured = columns
            .filter((c) => c.implementation !== "vqsort" || ["u32", "u64"].includes(ty))
            .filter((c) => c.implementation !== "radsort" || ty !== "owned")
            .map((c) => c.id);
          return {
            ty,
            distribution,
            len,
            operation: SORT,
            columns: measured,
            omitted: [],
            routes: Object.fromEntries(measured.map((id) => [id, "complete sorting call"])),
          };
        }),
    ),
  );
  const m: Manifest = {
    schema: 2,
    plan_name: "release",
    plan: {
      schema: 3,
      implementations: implementations.map((entry) => entry.name),
      include: [{ types, distributions, lengths }],
      exclude: [],
    },
    build: "default",
    blocks,
    cases,
    columns,
    commit: "fixture-only",
    dirty: false,
    rustc: "synthetic fixture; no performance evidence",
    source_sha256: "0".repeat(64),
    binary_sha256: "1".repeat(64),
    cargo_lock: '[[package]]\nname = "radsort"\nversion = "0.1.1"\n',
    features: ["COMPARE"],
    target: "x86_64-unknown-linux-gnu",
    profile: "release",
    opt_level: "3",
    debug: "false",
  };
  const dataset: Dataset = {
    host: {
      cpu: "Synthetic fixture",
      governor: "performance",
      turbo: "off",
      smt: "0",
      affinity: "2",
    },
    manifest: m,
    samples: [],
    memory: [],
    observations: [],
    partial: false,
  };
  const groups = new Set<string>();
  for (const c of cases) {
    const recordBytes = catalog.types.find((record) => record.id === c.ty)?.bytes;
    if (recordBytes === undefined) throw new Error(`Missing catalog width for ${c.ty}`);
    const group = caseGroup(c);
    groups.add(group);
    const factors: Record<string, number> = {
      "tessera/serial": 1,
      "std-stable/serial": 2.6,
      "radsort/serial": 1.3,
      "glidesort-sqrt/serial": 2.62,
      "voracious/serial": 1.301,
      "std-unstable/serial": 1.15,
      "vqsort/serial": 0.92,
    };
    if (c.distribution === "reverse") factors["radsort/serial"] = 0.7;
    if (c.distribution === "runs") factors["radsort/serial"] = 1.001;
    for (let block = 0; block < blocks; block++) {
      for (const [slot, column] of [...c.columns, TESSERA_AA].entries()) {
        const factor = column === TESSERA_AA ? (block % 2 ? 1.005 : 0.995) : factors[column];
        const sample: Sample = {
          kind: "sample",
          case_id: caseId(c),
          column,
          build: "default",
          group,
          block,
          slot,
          batch: 3,
          elapsed_ns: Math.round(c.len * 3 * 4 * factor * (1 + block / 100)),
          verified: true,
        };
        dataset.samples.push(sample);
      }
    }
    for (const column of c.columns) {
      if (c.len > 1000000 && c.ty !== "u64" && column !== TESSERA) continue;
      dataset.memory.push({
        case_id: caseId(c),
        column,
        build: "default",
        peak_extra_bytes:
          column === TESSERA || column === "glidesort-sqrt/serial"
            ? Math.ceil(Math.sqrt(c.len)) * recordBytes * 16
            : column === "vqsort/serial" || column === "std-unstable/serial"
              ? 0
              : c.len * recordBytes,
        verified: true,
      });
    }
  }
  for (const group of groups)
    for (const phase of ["start", "end"] as const) {
      dataset.observations.push({
        kind: "observation",
        group,
        build: "default",
        phase,
        unix_seconds: phase === "start" ? 100 : 200,
        load: "0.1 0.1 0.1",
        frequency_khz: "3000000",
        temperatures_millidegrees: [45000],
        memory_pressure: "some avg10=0.00",
      });
    }
  return dataset;
}

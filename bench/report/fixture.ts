// SPDX-License-Identifier: BSD-2-Clause

import { catalog } from "../catalog.ts";
import {
  caseId,
  type Dataset,
  lengths,
  type Manifest,
  type Sample,
  shapes,
  types,
} from "./model.ts";

/** Builds a synthetic release run with Tessera between faster and slower competitors. */
export function createFixture(): Dataset {
  const variants = [
    { name: "tessera", stable: true },
    { name: "std-stable", stable: true },
    { name: "radsort", stable: true },
    { name: "glidesort-sqrt", stable: true },
    { name: "long-stable-variant-name", stable: true },
    { name: "std-unstable", stable: false },
    { name: "vqsort", stable: false },
  ];
  const cases = types.flatMap((ty) =>
    shapes.flatMap((distribution) =>
      lengths
        .filter((n) => n <= 1000000 || !["record128", "owned"].includes(ty))
        .map((len) => ({
          ty,
          distribution,
          len,
          variants: variants
            .filter((v) => v.name !== "vqsort" || ["u32", "u64"].includes(ty))
            .filter((v) => v.name !== "radsort" || ty !== "owned")
            .map((v) => v.name),
        })),
    ),
  );
  const m: Manifest = {
    schema: 1,
    suite: "release",
    suite_version: 1,
    build: "default",
    blocks: 9,
    cases,
    variants,
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
    manifests: new Map([["default", m]]),
    samples: [],
    memory: [],
    observations: [],
    partial: false,
  };
  const groups = new Set<string>();
  for (const c of cases) {
    const recordBytes = catalog.types.find((record) => record.id === c.ty)?.bytes;
    if (recordBytes === undefined) throw new Error(`Missing catalog width for ${c.ty}`);
    const group = `${c.ty}/${c.distribution}`;
    groups.add(group);
    const factors: Record<string, number> = {
      tessera: 1,
      "std-stable": 2.6,
      radsort: 1.3,
      "glidesort-sqrt": 2.62,
      "long-stable-variant-name": 1.301,
      "std-unstable": 1.15,
      vqsort: 0.92,
    };
    if (c.distribution === "reverse") factors.radsort = 0.7;
    if (c.distribution === "runs") factors.radsort = 1.001;
    for (let block = 0; block < 9; block++) {
      for (const [slot, variant] of [...c.variants, "tessera-aa"].entries()) {
        const factor = variant === "tessera-aa" ? (block % 2 ? 1.005 : 0.995) : factors[variant];
        const sample: Sample = {
          kind: "sample",
          case_id: caseId(c),
          variant,
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
    for (const variant of c.variants) {
      if (c.len > 1000000 && c.ty !== "u64" && variant !== "tessera") continue;
      dataset.memory.push({
        case_id: caseId(c),
        variant,
        build: "default",
        peak_extra_bytes:
          variant === "tessera" || variant === "glidesort-sqrt"
            ? Math.ceil(Math.sqrt(c.len)) * recordBytes * 16
            : variant === "vqsort" || variant === "std-unstable"
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

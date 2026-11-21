// SPDX-License-Identifier: BSD-2-Clause
import { pairedRun } from '../continuous/fixtures/paired.ts';
import { measureSamples } from './samples.ts';
import { measureSamples as oldSchedule } from './replay-old.ts';
import { resolve } from 'node:path';
const [schedule, comparison] = process.argv[2].split('-');
const run = pairedRun();
run.cases = ['rust/u64/random/48/s1/api', 'rust/u64/random/49/s1/api', 'rust/row_u32/random/49/s1/api'];
const baseline = resolve('binaries/baseline-default/worker');
const current = comparison === 'aa' ? baseline : resolve('binaries/current-default/worker');
await measureSamples(run, { baseline, current }, resolve('plan.json'), process.argv[3], async () => {});
console.log(JSON.stringify(run.samples));

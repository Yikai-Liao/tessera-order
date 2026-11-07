// SPDX-License-Identifier: BSD-2-Clause
import { blockLsd, instabilities, lsd, msdInPlace, tessera } from './radixVariants.ts'
import { comparisonInput } from './comparisonInput.ts'

const input = comparisonInput
const sorted = (items: { key: number }[]) => items.every((x, i) => i === 0 || items[i - 1].key <= x.key)
for (const [name, run] of [['lsd', lsd], ['block-lsd', blockLsd], ['msd', msdInPlace], ['tessera', tessera]] as const) {
  const last = run(input).at(-1)!.items
  if (!sorted(last)) throw new Error(`${name} did not sort`)
  const unstable = instabilities(last).size
  if ((name === 'msd') !== (unstable > 0)) throw new Error(`${name}: ${unstable} unstable positions`)
}
console.log('radix variants ok; in-place MSD reorders equal keys on the comparison input')

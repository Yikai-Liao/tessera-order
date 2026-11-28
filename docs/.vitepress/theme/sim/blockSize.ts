// SPDX-License-Identifier: BSD-2-Clause

/**
 * Single authority for Tessera's serial block-partition sizing and memory model,
 * matching `default_block_size` in `rust/tessera-order/src/engine/block.rs` and
 * `finish::serial_descriptor_bytes` on 64-bit targets.
 */

export const DESCRIPTOR_BYTES = 52;
export const TILE_BYTES = 64 * 1024;

export function balancedBlockSize(records: number, buckets: number, recordBytes: number): number {
  return Math.ceil(Math.sqrt((DESCRIPTOR_BYTES * records) / (2 * buckets * recordBytes)));
}

export function chosenBlockSize(records: number, buckets: number, recordBytes: number): number {
  return Math.max(
    1,
    2 * balancedBlockSize(records, buckets, recordBytes),
    Math.floor(1024 / recordBytes),
    32,
  );
}

/** Memory at a candidate block size, or at the engine's chosen size when omitted. */
export function blockPartitionMemory(
  records: number,
  buckets: number,
  recordBytes: number,
  blockSize = chosenBlockSize(records, buckets, recordBytes),
) {
  const scratchBytes = 2 * buckets * blockSize * recordBytes;
  const ledgerBytes = (DESCRIPTOR_BYTES * records) / blockSize;
  const totalBytes = scratchBytes + ledgerBytes;
  const lsdBytes = records * recordBytes;
  return { blockSize, scratchBytes, ledgerBytes, totalBytes, lsdBytes };
}

export function formatBytesBinary(value: number): string {
  const units = ["B", "KiB", "MiB", "GiB"];
  let current = value;
  let unit = 0;
  while (current >= 1024 && unit < units.length - 1) {
    current /= 1024;
    unit++;
  }
  return `${current < 10 && !Number.isInteger(current) ? current.toFixed(1) : Math.round(current)} ${units[unit]}`;
}

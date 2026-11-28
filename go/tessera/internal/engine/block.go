// SPDX-License-Identifier: BSD-2-Clause

package engine

import "math"

// Block partition follows the Clausecker block layout used by Tessera's Rust
// port. Retain the upstream license notice below.
// Copyright (c) 2025, 2026 Robert Clausecker <clausecker@zib.de>
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
// IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
// TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
// PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//

const (
	// Widest digit accepted by a block partition.
	maxRadixBits = 12
	// Amortize the copy and metadata over both bytes and records.
	blockBytesFloor   = 1024
	blockRecordsFloor = 32
	blockScale        = 2
)

// Minimize 2*buckets*block*recordBytes + directoryBytes*length/block,
// then scale the balanced size to amortize random block reads.
func blockSize(length, recordBytes, buckets, directoryBytes int) int {
	tables := float64(directoryBytes) * float64(length)
	scratchPerRecord := 2 * float64(buckets) * float64(recordBytes)
	balanced := int(math.Ceil(math.Sqrt(tables / scratchPerRecord)))
	return max(balanced*blockScale, blockBytesFloor/recordBytes, blockRecordsFloor)
}

type bucketWriter struct{ next, end int }

type blockBuffers[T any] struct {
	scratch        []T
	links          []uint32
	writers        []bucketWriter
	writerPhysical []uint32
}

// Scatter consumes contiguous input and links each bucket's output blocks.
type partition[T any] struct {
	input                    []T
	buffers                  *blockBuffers[T]
	blockSize, scratchBlocks int
	inputBlock, outputBlocks int
}

func newPartition[T any](input []T, bucketCount, size int, buffers *blockBuffers[T]) partition[T] {
	scratchBlocks := 2 * bucketCount
	buffers.scratch = reserve(buffers.scratch, scratchBlocks*size)
	buffers.links = reserve(buffers.links, scratchBlocks+len(input)/size)
	buffers.writers = reserve(buffers.writers, bucketCount)
	buffers.writerPhysical = reserve(buffers.writerPhysical, bucketCount)
	for bucket := range buffers.writers {
		buffers.writers[bucket] = bucketWriter{next: bucket * size, end: (bucket + 1) * size}
		buffers.writerPhysical[bucket] = uint32(bucket)
	}
	return partition[T]{input: input, buffers: buffers, blockSize: size, scratchBlocks: scratchBlocks}
}

func (p *partition[T]) block(physical int) []T {
	if physical < p.scratchBlocks {
		start := physical * p.blockSize
		return p.buffers.scratch[start : start+p.blockSize]
	}
	start := (physical - p.scratchBlocks) * p.blockSize
	return p.input[start : start+p.blockSize]
}

// A block can be reused only after its last original record has been read.
//
//go:noinline
func (p *partition[T]) nextBlock(bucket int) {
	physical := p.scratchBlocks + p.outputBlocks
	p.buffers.links[p.buffers.writerPhysical[bucket]] = uint32(physical)
	p.buffers.writerPhysical[bucket] = uint32(physical)
	p.outputBlocks++
	p.buffers.writers[bucket] = bucketWriter{next: physical * p.blockSize, end: (physical + 1) * p.blockSize}
}

func (p *partition[T]) write(bucket int, record T) {
	writer := &p.buffers.writers[bucket]
	if writer.next < len(p.buffers.scratch) {
		p.buffers.scratch[writer.next] = record
	} else {
		p.input[writer.next-len(p.buffers.scratch)] = record
	}
	writer.next++
	if writer.next == writer.end {
		p.nextBlock(bucket)
	}
}

type blockClassifier[V encoded] struct {
	digit        Digit
	equal, split V
	rank         bool
}

func (c blockClassifier[V]) bucket(key V) int {
	if !c.rank {
		return keyDigit(key, c.digit)
	}
	bucket := 0
	if !less(key, c.equal) {
		bucket++
	}
	if less(c.equal, key) {
		bucket++
	}
	if !less(key, c.split) {
		bucket++
	}
	return bucket
}

// Keep setup and completion out of the scatter's optimization region.
//
//go:noinline
func scatterBlocks[T any, V encoded](p *partition[T], classifier blockClassifier[V], e *extractor[T, V]) {
	// Four records per bucket-cursor reload, scaled down for wide records.
	group := max(1, min(4, 4*(wordBits/8)/max(recordBytes[T](), 1)))
	mask := len(p.buffers.writers) - 1
	for p.inputBlock*p.blockSize < len(p.input) {
		start := p.inputBlock * p.blockSize
		source := p.input[start:min(start+p.blockSize, len(p.input))]
		complete := len(source) / group * group
		for start := 0; start < complete; start += group {
			for i := start; i < start+group; i++ {
				bucket := classifier.bucket(e.get(&source[i])) & mask
				if observeRoutes {
					emitRoute(routeEvent{stage: routeScatter, a: bucket})
				}
				p.write(bucket, source[i])
			}
		}
		for i := complete; i < len(source); i++ {
			bucket := classifier.bucket(e.get(&source[i])) & mask
			if observeRoutes {
				emitRoute(routeEvent{stage: routeScatter, a: bucket})
			}
			p.write(bucket, source[i])
		}
		p.inputBlock++
	}
}

type visitBand[T any] func(bucket, start, end int, band []T)

func partitionBlocks[T any, V encoded](input []T, radix uint, size int, buffers *blockBuffers[T], classifier blockClassifier[V], e *extractor[T, V], visit visitBand[T]) []int {
	p := newPartition(input, 1<<radix, size, buffers)
	scatterBlocks(&p, classifier, e)
	return completeSerial(&p, visit)
}

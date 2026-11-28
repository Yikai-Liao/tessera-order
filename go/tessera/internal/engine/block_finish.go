// SPDX-License-Identifier: BSD-2-Clause

package engine

const (
	freeBlock = -1
	// A final block has no unread source. Its other band fragments may
	// still be unwritten.
	finalBlock = -2
)

type blockSource struct {
	physical, length int
}

type blockBand struct {
	first, end int
	start      int
}

type completion[T any] struct {
	input                    []T
	partitions               []partition[T]
	blockSize, scratchBlocks int
	owners                   []int
	sources                  []blockSource
	bands                    []blockBand
	ends                     []int
	free                     []int
}

func serialDescriptorBytes() int {
	return recordBytes[uint32]() + recordBytes[int]()*2 + recordBytes[blockSource]()
}

// Build the source ledger and compute every final range before movement.
func buildCompletion[T any](input []T, partitions []partition[T]) completion[T] {
	p := &partitions[0]
	buckets := len(p.buffers.writers)
	scratchBlocks := len(partitions) * p.scratchBlocks
	fullBlocks := len(input) / p.blockSize
	count := scratchBlocks + fullBlocks
	c := completion[T]{
		input: input, partitions: partitions, blockSize: p.blockSize, scratchBlocks: scratchBlocks,
		owners:  make([]int, count),
		sources: make([]blockSource, 0, fullBlocks+len(partitions)*buckets),
		bands:   make([]blockBand, buckets),
		ends:    make([]int, buckets),
		// Every physical block appears at most once. Reserving all slots
		// keeps movement allocation-free.
		free: make([]int, 0, count),
	}
	for i := range c.owners {
		c.owners[i] = freeBlock
	}
	start := 0
	for bucket := range buckets {
		first := len(c.sources)
		length := 0
		firstBlock := 0
		// Preserve input order by visiting each bucket's shards in order.
		for shard := range partitions {
			part := &partitions[shard]
			writer := part.buffers.writers[bucket]
			tail := int(part.buffers.writerPhysical[bucket])
			for local := bucket; ; local = int(part.buffers.links[local]) {
				prefix := c.blockSize
				if local == tail {
					prefix -= writer.end - writer.next
				}
				if prefix != 0 {
					physical := shard*p.scratchBlocks + local
					if local >= p.scratchBlocks {
						physical = scratchBlocks + firstBlock + local - p.scratchBlocks
					}
					c.owners[physical] = len(c.sources)
					if observeRoutes {
						emitRoute(routeEvent{stage: routeSource, a: len(c.sources), b: prefix})
					}
					c.sources = append(c.sources, blockSource{physical, prefix})
					length += prefix
				}
				if local == tail {
					break
				}
			}
			firstBlock += len(part.input) / c.blockSize
		}
		c.bands[bucket] = blockBand{first, len(c.sources), start}
		start += length
		c.ends[bucket] = start
	}
	if start != len(input) {
		panic("tessera: incomplete block ledger")
	}
	for physical, owner := range c.owners {
		if owner == freeBlock {
			c.free = append(c.free, physical)
		}
	}
	return c
}

func (c *completion[T]) block(physical int) []T {
	if physical < c.scratchBlocks {
		perShard := c.partitions[0].scratchBlocks
		return c.partitions[physical/perShard].block(physical % perShard)
	}
	start := (physical - c.scratchBlocks) * c.blockSize
	return c.input[start : start+c.blockSize]
}

// Protect a destination by relocating its unread source if necessary.
func (c *completion[T]) protect(physical int) {
	logical := c.owners[physical]
	if logical >= 0 {
		var free int
		for {
			last := len(c.free) - 1
			free = c.free[last]
			c.free = c.free[:last]
			if c.owners[free] == freeBlock {
				break
			}
		}
		length := c.sources[logical].length
		copy(c.block(free)[:length], c.block(physical)[:length])
		c.owners[free] = logical
		c.sources[logical].physical = free
	}
	c.owners[physical] = finalBlock
}

// Protect each covered output block except the source itself. When the
// source overlaps the output, memmove transfers it before it becomes final.
func (c *completion[T]) transfer(logical, start int) int {
	source := c.sources[logical]
	if observeRoutes {
		emitRoute(routeEvent{stage: routeTransfer, a: logical})
	}
	first := start / c.blockSize
	last := min((start+source.length-1)/c.blockSize+1, len(c.input)/c.blockSize)
	overlaps := false
	for block := first; block < last; block++ {
		physical := c.scratchBlocks + block
		if physical == source.physical {
			overlaps = true
		} else {
			c.protect(physical)
		}
	}
	copy(c.input[start:start+source.length], c.block(source.physical)[:source.length])
	if overlaps {
		c.owners[source.physical] = finalBlock
	} else {
		c.owners[source.physical] = freeBlock
		c.free = append(c.free, source.physical)
	}
	return source.length
}

// Stream one bucket into its final band before publishing that band.
func (c *completion[T]) moveBand(bucket int) {
	band := c.bands[bucket]
	start := band.start
	for logical := band.first; logical < band.end; logical++ {
		start += c.transfer(logical, start)
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeBand, a: bucket, b: start - band.start})
	}
}

func completeSerial[T any](p *partition[T], visit visitBand[T]) []int {
	c := buildCompletion(p.input, []partition[T]{*p})
	for bucket, band := range c.bands {
		if band.first == band.end {
			continue
		}
		c.moveBand(bucket)
		if visit != nil {
			end := c.ends[bucket]
			visit(bucket, band.start, end, c.input[band.start:end:end])
		}
	}
	return c.ends
}

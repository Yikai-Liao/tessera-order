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
	partition *partition[T]
	owners    []int
	sources   []blockSource
	bands     []blockBand
	ends      []int
	free      []int
}

func serialDescriptorBytes() int {
	return recordBytes[uint32]() + recordBytes[int]()*2 + recordBytes[blockSource]()
}

// Build the source ledger and compute every final range before movement.
func buildCompletion[T any](p *partition[T]) completion[T] {
	buckets := len(p.buffers.writers)
	count := len(p.buffers.links)
	c := completion[T]{
		partition: p,
		owners:    make([]int, count),
		sources:   make([]blockSource, 0, len(p.input)/p.blockSize+buckets),
		bands:     make([]blockBand, buckets),
		ends:      make([]int, buckets),
		// Every physical block appears at most once. Reserving all slots
		// keeps movement allocation-free.
		free: make([]int, 0, count),
	}
	for i := range c.owners {
		c.owners[i] = freeBlock
	}
	start := 0
	for bucket, writer := range p.buffers.writers {
		first := len(c.sources)
		tail := int(p.buffers.writerPhysical[bucket])
		length := 0
		for physical := bucket; ; physical = int(p.buffers.links[physical]) {
			prefix := p.blockSize
			if physical == tail {
				prefix -= writer.end - writer.next
			}
			if prefix != 0 {
				c.owners[physical] = len(c.sources)
				if observeRoutes {
					emitRoute(routeEvent{stage: routeSource, a: len(c.sources), b: prefix})
				}
				c.sources = append(c.sources, blockSource{physical, prefix})
				length += prefix
			}
			if physical == tail {
				break
			}
		}
		c.bands[bucket] = blockBand{first, len(c.sources), start}
		start += length
		c.ends[bucket] = start
	}
	if start != len(p.input) {
		panic("tessera: incomplete block ledger")
	}
	for physical, owner := range c.owners {
		if owner == freeBlock {
			c.free = append(c.free, physical)
		}
	}
	return c
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
		p := c.partition
		length := c.sources[logical].length
		copy(p.block(free)[:length], p.block(physical)[:length])
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
	p := c.partition
	first := start / p.blockSize
	last := min((start+source.length-1)/p.blockSize+1, len(p.input)/p.blockSize)
	overlaps := false
	for block := first; block < last; block++ {
		physical := p.scratchBlocks + block
		if physical == source.physical {
			overlaps = true
		} else {
			c.protect(physical)
		}
	}
	copy(p.input[start:start+source.length], p.block(source.physical)[:source.length])
	if overlaps {
		c.owners[source.physical] = finalBlock
	} else {
		c.owners[source.physical] = freeBlock
		c.free = append(c.free, source.physical)
	}
	return source.length
}

// Stream one bucket into its final band before publishing that band.
func (c *completion[T]) finish(bucket int, visit visitBand[T]) {
	band := &c.bands[bucket]
	if band.first == band.end {
		return
	}
	start := band.start
	for logical := band.first; logical < band.end; logical++ {
		start += c.transfer(logical, start)
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeBand, a: bucket, b: start - band.start})
	}
	if visit != nil {
		input := c.partition.input
		visit(bucket, band.start, start, input[band.start:start:start])
	}
}

func completeSerial[T any](p *partition[T], visit visitBand[T]) []int {
	c := buildCompletion(p)
	for bucket := range c.bands {
		c.finish(bucket, visit)
	}
	return c.ends
}

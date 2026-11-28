// SPDX-License-Identifier: BSD-2-Clause

package engine

import "sync"

func partitionParallel[T any, V encoded](records []T, radix uint, buffers *blockBuffers[T], classifier blockClassifier[V], e *extractor[T, V], shards, workers int, visit func(worker, bucket int, band []T)) {
	buckets := 1 << radix
	shards = min(max(shards, 1), len(records))
	size := blockSize(len(records), recordBytes[T](), shards*buckets, serialDescriptorBytes())
	fullBlocks := len(records) / size
	shards = min(shards, max(fullBlocks, 1))
	if shards == 1 {
		size = blockSize(len(records), recordBytes[T](), buckets, serialDescriptorBytes())
		if observeRoutes {
			emitRoute(routeEvent{stage: routeBlock, a: len(records), b: int(radix), c: size})
			emitRoute(routeEvent{stage: routeShards, a: 1, b: size})
		}
		partitionBlocks(records, radix, size, buffers, classifier, e, func(bucket, _, _ int, band []T) {
			visit(0, bucket, band)
		})
		return
	}
	if observeRoutes {
		emitRoute(routeEvent{stage: routeBlock, a: len(records), b: int(radix), c: size})
		emitRoute(routeEvent{stage: routeShards, a: shards, b: size, c: 1})
	}

	// Every shard starts on a full block. Only the final one owns the tail.
	partitions := make([]partition[T], shards)
	locals := make([]blockBuffers[T], shards-1)
	first := 0
	for shard := range partitions {
		blocks := fullBlocks / shards
		if shard < fullBlocks%shards {
			blocks++
		}
		end := first + blocks*size
		local := buffers
		if shard != 0 {
			local = &locals[shard-1]
		}
		if shard == shards-1 {
			end = len(records)
		}
		partitions[shard] = newPartition(records[first:end:end], buckets, size, local)
		first = end
	}
	runTasks(workers, shards, func(_, shard int) {
		scatterBlocks(&partitions[shard], classifier, e)
	})

	c := buildCompletion(records, partitions)
	var ledger sync.Mutex
	// Movement shares a free stack and source locations. Once a band is
	// final, peers can only write disjoint fragments and cannot read it.
	move := func(bucket int) {
		ledger.Lock()
		defer ledger.Unlock()
		c.moveBand(bucket)
	}
	runTasks(workers, buckets, func(worker, bucket int) {
		band := c.bands[bucket]
		if band.first == band.end {
			return
		}
		move(bucket)
		end := c.ends[bucket]
		visit(worker, bucket, records[band.start:end:end])
	})
}

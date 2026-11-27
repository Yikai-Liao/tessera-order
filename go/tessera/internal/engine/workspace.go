// SPDX-License-Identifier: BSD-2-Clause

package engine

// Workspace reuses capacity across the nodes and tiles of one refinement.
type workspace[T any] struct {
	tile  tileScratch[T]
	block blockBuffers[T]
}

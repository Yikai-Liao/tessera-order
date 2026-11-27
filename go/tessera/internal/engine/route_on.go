// SPDX-License-Identifier: BSD-2-Clause

//go:build tessera_trace

package engine

import "sync"

const observeRoutes = true

var routeLock sync.Mutex
var routeObserver func(routeEvent)

func emitRoute(event routeEvent) {
	if routeObserver != nil {
		routeLock.Lock()
		defer routeLock.Unlock()
		routeObserver(event)
	}
}

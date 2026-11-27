// SPDX-License-Identifier: BSD-2-Clause

//go:build tessera_trace

package engine

const observeRoutes = true

var routeObserver func(routeEvent)

func emitRoute(event routeEvent) {
	if routeObserver != nil {
		routeObserver(event)
	}
}

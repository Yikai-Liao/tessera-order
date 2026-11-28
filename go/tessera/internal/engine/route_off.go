// SPDX-License-Identifier: BSD-2-Clause

//go:build !tessera_trace

package engine

const observeRoutes = false

func emitRoute(routeEvent) {}

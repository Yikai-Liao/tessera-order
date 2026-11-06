# 0006: Guard record movement against unwinding without std

Date: 2026-11-07. Status: accepted. Supersedes the panic-strategy requirement
and safety rationale in [0005](0005-no-std-with-alloc.md).

## Context

`no_std` does not select a panic strategy. A dependency built without its `std`
feature can still run in a host program that unwinds. Requiring `panic = "abort"`
in documentation does not protect a safe API from such a caller.

During block scatter, consumed input blocks become output blocks before the
ownership directory is rebuilt. If unwinding interrupts that phase, the
pass-boundary `Drop` restoration uses an incomplete directory and can duplicate
owning records, causing a double free. This is a memory-safety issue, not merely
a leak. A callback panic during block materialization likewise must not unwind
through an incomplete directory.

## Decision

Keep the `std` recovery path unchanged. In the no-std implementation of
`unwind::catch`, place an abort-on-unwind guard around the closure. Forget the
guard after a normal return. If the closure unwinds, the guard's destructor
panics during cleanup, which aborts before the block sorter's destructor can
attempt restoration. This uses only `core`, adds no dependency, and works with
both aborting and unwinding hosts.

The guard covers block scatter and its materialization callbacks. Key-function
panics elsewhere may unwind safely through the existing tile restoration or
before records are moved. The public contract documents this distinction;
applications need not choose a particular panic strategy.

Use the integer `isqrt` method from `core`, stable since Rust 1.84 and available
at the crate's Rust 1.85 MSRV, for no-std block sizing. A single conditional
increment rounds its floor result up. Keep the measured `std` floating-point
formula and the comparison between both forms.

## Alternatives considered

- **Reject no-std builds with `cfg(panic = "unwind")`.** This prevents the unsafe
  combination, but also rejects host uses and the normal stable test harness.
  A guard protects the actual ownership boundary without this restriction.
- **Repair interrupted scatter in `Drop`.** This needs the scatter cursors and
  completion protocol restructured. It is unnecessary for an abort-on-panic
  contract and would change the existing recovery mechanism.

## Consequences

- A subprocess regression test verifies that a key panic during block scatter
  aborts under the host unwinder for both free functions and a reused workspace.
  The fallback exits without dropping possibly duplicated owning records, so a
  double free cannot masquerade as successful abort protection.
- Inconsistent-key tests for owning and exclusively borrowed records run with
  and without `std`; only panic-recovery cases require the feature. Miri runs
  both feature configurations, excluding the subprocess test.
- Square-boundary tests check integer rounding independently of the floating
  comparison. No routing, thresholds, or `std` block sizes change.

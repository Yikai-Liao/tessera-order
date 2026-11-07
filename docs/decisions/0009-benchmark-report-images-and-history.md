# 0009: Publish immutable report images and compare PR history with main

Date: 2026-11-07. Status: accepted.

## Context

Actions artifacts provide ZIP downloads rather than public image endpoints,
so an SVG artifact does not appear in a PR comment. PR benchmark heads are
reset when their main start point changes, and a PR may be rebased or
force-pushed. Querying only the current head's performance series would lose
earlier PR measurements.

A PR's history should show what reviewers need before merging: how far each
run was from main, including a regression introduced and fixed within the PR.
Adding benchmark cases must not produce a jump that looks like a PR
optimization.

## Decision

Keep current comparison, PR history, SVG rendering and GitHub publication as
separate operations in the existing report pipeline. The two SVG renderers
share colours, text metrics and typography. Report publication passes their
image URLs to the Markdown renderer.

Publish generated SVGs to the image-only `benchmark-images` branch through
the Git data API. Create its first commit without source-tree parents;
subsequent commits retain other reports' images. Embed them from
`raw.githubusercontent.com` at the image commit. Do not upload them as
artifacts. Only the trusted tracking job receives repository content-write
permission. Its concurrency group serializes writers, and branch updates are
not forced.

Load every completed report for the PR's current Testbed and hardware spec,
including earlier reset heads and same-commit reruns. Compare every run with
main's baseline from the latest report, over one fixed set of cases that have
that baseline and a value in every run. Select rows by their swing between PR
runs and by current regression status, with a threshold separate from the
comparison chart's, as specified in the
[benchmark guide](../development/continuous-benchmarks.md#pr-history).

Preserve the current measurement and its alert verdict if supplemental
history or image publication is unavailable. Log the failure and link it in
the report footer.

## Consequences

Reports embed images without adding derived files to main. Older comments
keep their original images. The image branch retains its history and grows
with the number of reports. If image publication fails, that report has no
chart; its tables still carry every number.

One reference for the whole history needs no knowledge of the PR's Git
topology: Bencher resolves main's baseline at the PR's current start point.
When main moves, earlier runs are compared with the new main, which is the
branch the PR would merge into. Cases new in the PR have no baseline and do
not appear in the history chart.

The complete-history query is paginated and time-bounded to the current
report. A case's changed input still requires a new benchmark ID; compiler
or environment changes still require a new Testbed.

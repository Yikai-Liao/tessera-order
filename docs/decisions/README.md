# Decision records

Each file records one decision that shapes the project: the context, the decision and its consequences. Records are not rewritten when a decision changes; a new record supersedes the old one, and the old one says so at the top.

A record takes the number of the pull request that adds it, so parallel pull requests never claim the same number ([decision 0032](0032-parallel-pull-requests.md)); numbers have gaps. A pull request that adds several records gives them the suffixes `a`, `b`, `c`. The records are listed by number in this directory and in the documentation site's sidebar.

Historical experimental-branch records, not upstream pull-request decisions:

- [Optional bounded Rayon](0016-optional-bounded-rayon.md).
- [Materialized bucket overlap](experimental-materialized-bucket-overlap.md).

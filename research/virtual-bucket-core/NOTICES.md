# License and source notices

This is original BSD-2-Clause research code from the Tessera investigation.
Source files retain their SPDX headers. LICENSE preserves the parent project's
BSD-2-Clause notice and copyright lines without modification.

No third-party sorting implementation is vendored in this bundle. The owned-rank
crate uses the repository's Tessera serial crate by path and Rayon through Cargo;
the virtual-bucket foundation is dependency-free. Their respective upstream
license notices remain applicable to those dependencies.

The rank-partition concept was informed by public descriptions of DuckDB Merge
Path sorting; no DuckDB source code was copied. No production or general API
endorsement is implied by inclusion in a research checkpoint.

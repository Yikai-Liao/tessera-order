# Security policy

Tessera uses unsafe code to move records through uninitialized memory. Memory-safety bugs are treated as security issues: out-of-bounds access, use of uninitialized memory, double drops, records lost or duplicated, or undefined behavior reachable from safe code, including through key functions that panic or return inconsistent keys.

## Reporting

Please report vulnerabilities privately through [GitHub security advisories](https://github.com/Yikai-Liao/tessera-order/security/advisories/new). Do not open a public issue.

Include a reproduction if you can: the key and record types, the input or a program that generates it, and the toolchain.

## Supported versions

Before 1.0, only the latest release receives fixes.

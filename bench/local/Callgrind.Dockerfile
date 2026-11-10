# SPDX-License-Identifier: BSD-2-Clause

# A baseline-ISA loader also supports hosts whose libc requires AVX-512.
ARG BASE_IMAGE=ubuntu:24.04
FROM ${BASE_IMAGE}
RUN sed -i 's|http://|https://|g' /etc/apt/sources.list.d/*.sources \
    && apt-get update && apt-get install -y --no-install-recommends valgrind \
    && rm -rf /var/lib/apt/lists/*

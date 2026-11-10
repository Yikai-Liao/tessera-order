# SPDX-License-Identifier: BSD-2-Clause
FROM node:24-bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends util-linux \
    && rm -rf /var/lib/apt/lists/*
COPY default/bench /bench-default
COPY bench /bench
COPY tools /tools

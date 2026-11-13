# SPDX-License-Identifier: BSD-2-Clause
# Actions supplies frozen workers and inputs. Remote execution never compiles.
FROM alpine:3.23 AS static-runtime
RUN apk add --no-cache util-linux-misc setarch
WORKDIR /app
COPY . .
RUN chmod 755 /app/run.sh
CMD ["/app/run.sh", "0"]

# Clang-built native workers use libstdc++ and glibc, not a GCC build toolchain.
FROM ubuntu:24.04 AS native-runtime
RUN apt-get update && apt-get install -y --no-install-recommends libstdc++6 util-linux \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN chmod 755 /app/run.sh
CMD ["/app/run.sh", "0"]

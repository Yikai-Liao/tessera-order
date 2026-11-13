# SPDX-License-Identifier: BSD-2-Clause
# Actions supplies frozen binaries and inputs. Remote execution never builds or downloads sources.
FROM alpine:3.23 AS static-runtime
RUN apk add --no-cache util-linux-misc setarch
WORKDIR /app
COPY benchmark memory algorithm plan.json *.ids run.sh ./
RUN chmod 755 /app/run.sh
CMD ["/app/run.sh", "0"]

# Native adapters use the runner's GNU C++ ABI. Keep their runtime separate from static Rust images.
FROM ubuntu:24.04 AS native-runtime
RUN apt-get update && apt-get install -y --no-install-recommends libstdc++6 util-linux \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY benchmark memory algorithm plan.json *.ids run.sh ./
RUN chmod 755 /app/run.sh
CMD ["/app/run.sh", "0"]

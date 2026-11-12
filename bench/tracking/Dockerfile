# SPDX-License-Identifier: BSD-2-Clause
# CI supplies only the executable, plan, inventory and execution script.
FROM alpine:3.23
RUN apk add --no-cache util-linux-misc setarch
WORKDIR /app
ENV TESSERA_BENCH_PLAN=/app/plan.json
COPY benchmark plan.json api.ids par-4.ids run.sh ./
RUN chmod 755 /app/run.sh
CMD ["/app/run.sh"]

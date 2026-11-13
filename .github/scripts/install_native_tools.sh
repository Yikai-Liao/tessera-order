#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-2-Clause
set -euo pipefail
clang --version
clang++ --version
if dpkg-query -W -f='${Status}' libtbb-dev 2>/dev/null | grep -q 'install ok installed'; then
  exit 0
fi
# Isolate Ubuntu packages from unrelated runner repositories. The Azure
# mirror has recovered from the outage (actions/runner-images#15296).
mirror=https://azure.archive.ubuntu.com/ubuntu/
dir="$RUNNER_TEMP/apt"
mkdir -p "$dir/parts" "$dir/lists"
for suite in noble noble-updates noble-backports noble-security; do
  printf 'deb %s %s main restricted universe multiverse\n' "$mirror" "$suite"
done > "$dir/ubuntu.list"
apt=(sudo apt-get
  -o Acquire::Retries=2
  -o Acquire::http::Timeout=10
  -o Acquire::https::Timeout=10
  -o Dir::Etc::SourceList="$dir/ubuntu.list"
  -o Dir::Etc::SourceParts="$dir/parts"
  -o Dir::State::lists="$dir/lists")
"${apt[@]}" -o APT::Update::Error-Mode=any update
"${apt[@]}" install -y --no-install-recommends libtbb-dev

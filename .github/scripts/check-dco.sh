#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-2-Clause
#
# Fails unless every non-merge commit in BASE..HEAD has a Signed-off-by
# trailer naming the commit author's email, except verified Dependabot commits.
# Usage: check-dco.sh BASE HEAD
set -euo pipefail

base=$1
head=$2
missing=0

for commit in $(git rev-list --no-merges "$base..$head"); do
    email=$(git show -s --format='%ae' "$commit")
    signoffs=$(git show -s --format='%(trailers:key=Signed-off-by,valueonly)' "$commit")
    if ! grep -qiF "<$email>" <<<"$signoffs"; then
        # Author fields alone are forgeable. Require an authenticated bot
        # update of this exact head and GitHub's verified commit signature.
        if [[ "$email" == '49699333+dependabot[bot]@users.noreply.github.com' ]] &&
            python3 "$(dirname "$0")/verify_dependabot.py" "$commit"; then
            continue
        fi
        echo "::error::$(git show -s --format='%h %s' "$commit"): no Signed-off-by for the author <$email>"
        missing=1
    fi
done

if [[ $missing -ne 0 ]]; then
    echo "Sign off each commit with 'git commit -s'. See CONTRIBUTING.md, section 'Sign-off (DCO)'."
fi
exit "$missing"

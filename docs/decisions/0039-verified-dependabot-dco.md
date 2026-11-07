# 0039: Exempt verified Dependabot head commits from DCO

Date: 2026-11-07. Status: accepted.

## Context

Dependabot signs off as `dependabot[bot] <support@github.com>`, while its commit author uses `49699333+dependabot[bot]@users.noreply.github.com`. The ordinary DCO check requires an author-matching sign-off, so the bot's dependency updates fail it.

A bot-like author name or email is not authentication. GitHub's web-flow signature verifies a commit but does not by itself prove that the attributed author initiated it. Exempting an entire bot-created pull request would also exempt later human contributions.

## Decision

Exempt only the current head commit of a pull request that the official Dependabot account opened or synchronized. Require the authenticated event sender, initial workflow actor and pull request creator to identify that account. The head and base repositories must match the event repository; the checked SHA must equal the head SHA and, on synchronization, the event's `after` SHA.

For that exact SHA, also require GitHub's API to return the official Dependabot author account, the GitHub web-flow committer, their expected email addresses and a valid verified signature. Missing or malformed provenance and API failures grant no exception. Every other non-merge commit keeps the ordinary DCO requirement.

Run the checker and verification helper from the target branch. Fetch pull request objects for inspection without checking out or executing their contents. Keep the workflow token read-only and leave repository protection rules unchanged.

## Consequences

- Genuine Dependabot updates can pass DCO without rewriting their standard sign-offs or signatures.
- Human changes to a bot branch do not qualify for the exception. Ask Dependabot to rebase the branch so that it produces a new authenticated update.
- Re-running a bot-origin workflow keeps its initial actor; the identity of the person requesting the rerun does not grant or remove the exception.
- An unavailable verification API requires a later retry. Other contributors remain subject to matching sign-offs.
- This rule does not make pull-request-editable workflow definitions tamper-proof; workflow changes still require review.

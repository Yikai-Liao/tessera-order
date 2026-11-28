# Comparing and evolving designs

## Compare genuinely different candidates

For major, expensive, or hard-to-reverse decisions, compare viable options that change authority, representation, lifecycle, or call protocol: for new work, use a direct implementation as the reference; for a refactor, include the status quo or a local fix.

Compare caller knowledge, state, failure, performance, compatibility, and reversibility on the same workflow. Match the depth of comparison to the cost of the decision.

## Migrate by complete workflow

Settle rule and state ownership first, then cut the smallest complete slice that delivers current value and takes back responsibility.

When several known candidates reshape the same contract, representation, or area, settle its target structure from all of them before landing slices, so each slice moves toward that end state instead of a local optimum the next slice reshapes. Local cleanups inside an area awaiting such a change wait for it or fold into it.

- **Acceptance unit**: each step replaces one mechanism or changes one behavior or routing choice, evaluated against the preceding head; structural and behavior-changing steps stay separate. A step that bundles several replacements cannot attribute a failed check, so it can be neither accepted nor partially reverted.
- **Behavior**: separate formal guarantees, consumer dependencies, and unknowns; check ordering, defaults, errors, timing, parallelism policy, and performance; keep or migrate per the goal, and report actual semantic changes.
- **Steps**: migrate complete use cases using narrow seams where needed, retiring old entry points and temporary seams once they no longer serve a purpose; each step has a success condition and a recovery path; temporary dual paths state the authority, remaining consumers, and exit condition.
- **Data**: define read/write ownership before and after cutover, backfill or sync, and recovery; plan application rollback and data rollback separately.

When a bulk migration is worth automating, first complete and independently verify one representative complete slice, then have the tool reproduce the transformation; check boundaries, failures, and rerun semantics before widening the scope.

## Retire old mechanisms

After a responsibility or representation changes, search along the old concept for callers, fields, parameters, adapters, configuration, tests, and docs; remove interfaces, mirrored state, and assembly requirements that no longer have an independent purpose, and update the current contract description.

## Collaboration and final composition

Shared contracts are maintained by a clear authority. Coordinate schemas, generated artifacts, databases, and lifecycles by actual dependency: converge shared decisions first, then parallelize the implementations that depend on them.

## Verify behavior

Choose checks and verification seams for migration and final composition per [test protection and maintenance cost](design-behavioral-tests.md).

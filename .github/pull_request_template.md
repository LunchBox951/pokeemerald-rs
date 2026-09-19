<!--
PR structure baseline. Fill every section; delete the inline guidance, not the
headings. Keep it lean (lean-docs).
-->

## Summary

<!-- What changed and why, in a few lines. -->

## Linked issue

<!--
Closes #ISSUE and/or names the acceptance ID(s) it advances.
Cite the v1 acceptance ID this ladders up to (docs/acceptance/v1.md), e.g. F-7.
-->

- Issue:
- Acceptance ID:

## Test evidence

<!--
How this was verified — commands run and their outcome (test-ratchet).
-->

## Ledger impact

<!-- One of: none / verify-only / list of touched entries. -->

- [ ] none
- [ ] verify-only
- [ ] list:

## Dependency impact

<!-- One of: none / explicit owner-approved (link the approval) (minimal-deps). -->

- [ ] none
- [ ] explicit, owner-approved:

## Checklist

- [ ] No test deleted or weakened to pass a gate (test-ratchet).
- [ ] No upstream C copied verbatim; behaviour re-implemented (no-verbatim).
- [ ] No new Cargo dependency, or it is explicitly owner-approved (minimal-deps).

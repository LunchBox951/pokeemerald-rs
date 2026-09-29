# Security policy

`pokeemerald-rs` is a pre-alpha hobby reimplementation maintained on a best-effort basis. Security reports still receive private review.

## Reporting a vulnerability

Do not open a public issue. Use GitHub's **Security → Report a vulnerability** flow or open a [private advisory](https://github.com/LunchBox951/pokeemerald-rs/security/advisories/new).

Include the affected channel and `VERSION`, reproduction steps, observed impact, and any relevant logs. GitHub routes the report privately to the repository owner. The project publishes no email support address.

## Response

Reports are acknowledged and triaged without a fixed SLA. Automated dependency and code-scanning signals enter the same private review and normal fix path. Human-only decisions receive `needs-operator` instead of an automated guess.

Security fixes enter through `dev` and follow the direct ladder through `unstable`, `stable`, and `main`. Stable and main retain their objective gates, CODEOWNER approval, and manual merge requirements; urgency never creates a direct-to-main bypass.

The player channels are stable (`main`), beta (`stable`), and nightly (`unstable`). Older prerelease tags are not maintained separately.

## Scope

The repository ships no game assets or upstream code and links no upstream C `(no-verbatim, no-ffi)`. Reports about the read-only `pokeemerald/` or `mgba/` checkouts belong with those upstream projects `(reference-only)`.

# Security Policy

`pokeemerald-rs` is a pre-alpha, autonomously-maintained hobby reimplementation.
We still take security reports seriously and would rather hear about an issue
privately than read about it in a public thread.

## Reporting a vulnerability

**Please report privately. Do not open a public issue for a security problem.**

Use GitHub's private vulnerability reporting:

- Go to the repository's **Security** tab → **Report a vulnerability**, or
- Open a private advisory directly:
  <https://github.com/LunchBox951/pokeemerald-rs/security/advisories/new>

This routes the report to the maintainer (**@LunchBox951**) privately through
GitHub. There is no email contact — please use the GitHub channel above.

When you report, include what you'd want if you were fixing it: affected branch
or channel and commit/version, what the issue is, how to reproduce it, and the
impact you observed.

## What to expect

This project is maintained on a best-effort basis, largely by an automated
maintenance routine with owner oversight. Reports are acknowledged and triaged
when the routine next runs and the owner is available, not on a fixed SLA.
Genuinely owner-level calls are raised as `needs-operator` rather than guessed.

The maintenance routine also triages automated security signals — **Dependabot**
alerts and **CodeQL** code-scanning results — and folds them into the normal
issue/PR pipeline so dependency and code-scan findings are not lost between runs.

## Supported channels

Fixes flow down the release ladder. The supported player channels are:

| Channel | Branch | Audience |
|---------|--------|----------|
| stable  | `main`     | most stable release |
| beta    | `stable`   | broadly-validated beta |
| nightly | `unstable` | freshest playable build |

`dev` is the developer integration branch, not a player channel.

Serious player-facing and security defects take the expedited direct ladder: land
the minimal fix on `dev`, validate it, then promote `dev -> unstable -> stable ->
main`. Stable/main still require their objective gates and CODEOWNER approval, but
have no mandatory time delay. For an urgent candidate, an owner may manually
dispatch `promote` with `merge-unstable` once every nightly gate is green; prefer
the normal off-hours window when urgency permits. Older prerelease tags are not
separately maintained — upgrade to a current channel.

## Scope notes

This is a clean-room reimplementation that ships **no game assets** and **no
upstream code** `(no-verbatim)`, and links to **no upstream C** `(no-ffi)`.
Reports about the read-only upstream references under `pokeemerald/` or `mgba/`
belong with those projects, not here `(reference-only)`.

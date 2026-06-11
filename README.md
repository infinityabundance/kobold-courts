# kobold-courts

Court/casefile machinery: casefile schema, receipt doctrine, claim-ladder, negative-capability registry, DSSE/in-toto packet generation, court runners, diffing, and audit reports.

**Part of KOBOLD** -- a forensic archaeology and evidence system for legacy COBOL estates: it maps real COBOL
codebases, generated oracle witnesses, compiler-profile behavior, and migration risk into court-backed
receipts. Independently-authored tooling; contains no GnuCOBOL source.

## What it does (v0.1)
A reusable, generalized model of the court method -- it operates on ANY claim-ladder, not just gnucobol-rs's:
- `Court`, `ClaimLadder`, `Casefile`, `NegativeCapability`, `Violation`, `CourtSet` (serde types).
- `CourtSet::load() / validate()` -- enforces the trust invariant **negatives >= positives** per court
  (and unique ids). `Court::positive_claims()/negative_claims()` split `proven`/`not_proven` faithfully.
- `Casefile::from_court()` -- the binding forensic record per court.
- CLI: `kobold-courts validate|summary|casefiles <claim-ladder.json>`.
- **Forensic casefile generator** (`forensic` module; lab/casefile successor): `casefile generate|check
  --root <repo>` builds the rich machine-verifiable casefile (claims, non-claims, NEG ids, evidence hashes,
  replay, oracle profile, verdict from the TRUST.2 receipt) + casefile.md + sarif.json, and the `check`
  regenerates+diffs and enforces negatives>=positives / damage_if_overclaimed / receipt presence.

```
cargo run -- check --root path/to/repo        # verify <repo>/reports/claim-ladder.json
cargo run -- validate  path/to/claim-ladder.json
cargo run -- summary   path/to/claim-ladder.json
cargo run -- casefiles path/to/claim-ladder.json   # one JSON casefile per line
```

Roadmap: SARIF / in-toto / DSSE packet emission, court diffing, audit reports, receipt doctrine.

## Architecture
- gnucobol-rs (separate crate) = the oracle-proven semantic primitive layer.
- kobold-* = the forensic-intelligence layer.
- kobold-* MAY depend on gnucobol-rs; gnucobol-rs MUST NOT depend on kobold-*.

## License
Apache-2.0 (see LICENSE).

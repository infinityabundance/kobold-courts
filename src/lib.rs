#![forbid(unsafe_code)]
//! # kobold-courts
//!
//! The forensic proof method as a reusable Apache-2.0 library: a generalized court / casefile /
//! claim-ladder / negative-capability model that operates on ANY claim-ladder, not just gnucobol-rs's.
//!
//! Part of the KOBOLD ecosystem (independently-authored tooling; no GnuCOBOL source). Dependency rule:
//! kobold-* MAY depend on gnucobol-rs; gnucobol-rs MUST NOT depend on kobold-*.
//!
//! Core doctrine, enforced here: **negative capability is the trust surface** -- every court's negative
//! claims (its loud non-claims) must be at least as many as its positive claims.

use serde::{Deserialize, Serialize};

pub mod forensic;
use std::collections::HashSet;
use std::path::Path;

/// One court: a sealed (or candidate) claim paired with its loud non-claim.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Court {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub proven: String,
    #[serde(default)]
    pub byte_domain: String,
    #[serde(default)]
    pub oracle: String,
    #[serde(default)]
    pub fixtures: String,
    #[serde(default)]
    pub sealed_version: String,
    #[serde(default)]
    pub not_proven: String,
    #[serde(default)]
    pub breaks_claim: String,
    #[serde(default)]
    pub readiness: i64,
    #[serde(default)]
    pub lie_prevented: String,
    #[serde(default)]
    pub damage_if_overclaimed: String,
}

impl Court {
    /// Positive claims = `proven` split on `;` or ` + ` (when the following token starts uppercase or `(`),
    /// matching the reference casefile generator. ASCII-boundary safe (no panics on unicode).
    pub fn positive_claims(&self) -> Vec<String> {
        split_positives(&self.proven)
    }

    /// Negative claims = `not_proven` split on `;`, plus the `lie_prevented` line when present.
    pub fn negative_claims(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .not_proven
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        let lp = self.lie_prevented.trim();
        if !lp.is_empty() {
            v.push(format!("lie prevented: {lp}"));
        }
        v
    }

    /// The trust invariant: negative capability is the trust surface.
    pub fn negatives_ge_positives(&self) -> bool {
        self.negative_claims().len() >= self.positive_claims().len()
    }

    /// `observed-atlas` for ATLAS courts, else `court`.
    pub fn kind(&self) -> &'static str {
        if self.id.contains("ATLAS") {
            "observed-atlas"
        } else {
            "court"
        }
    }
}

fn split_positives(proven: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in proven.split(';') {
        let mut start = 0usize;
        for (idx, _) in chunk.match_indices(" + ") {
            if let Some(c) = chunk[idx + 3..].chars().next() {
                if c.is_ascii_uppercase() || c == '(' {
                    let piece = chunk[start..idx].trim();
                    if !piece.is_empty() {
                        out.push(piece.to_string());
                    }
                    start = idx + 3;
                }
            }
        }
        let piece = chunk[start..].trim();
        if !piece.is_empty() {
            out.push(piece.to_string());
        }
    }
    if out.is_empty() && !proven.trim().is_empty() {
        out.push(proven.trim().to_string());
    }
    out
}

/// A claim-ladder: the ordered set of courts plus its doctrine header.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaimLadder {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub doctrine: String,
    #[serde(default)]
    pub oracle: String,
    pub courts: Vec<Court>,
}

/// A generated forensic casefile (the binding record for one court).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Casefile {
    pub schema: String,
    pub case_id: String,
    pub kind: String,
    pub positive_claims: Vec<String>,
    pub negative_claims: Vec<String>,
    pub readiness: i64,
    pub negatives_ge_positives: bool,
}

impl Casefile {
    pub fn from_court(c: &Court) -> Self {
        Casefile {
            schema: "kobold-casefile-v1".into(),
            case_id: c.id.clone(),
            kind: c.kind().into(),
            positive_claims: c.positive_claims(),
            negative_claims: c.negative_claims(),
            readiness: c.readiness,
            negatives_ge_positives: c.negatives_ge_positives(),
        }
    }
}

/// A validation finding.
#[derive(Debug, Clone)]
pub struct Violation {
    pub court_id: String,
    pub kind: String,
    pub detail: String,
}

/// A loaded, queryable court set.
pub struct CourtSet {
    pub ladder: ClaimLadder,
}

impl CourtSet {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        Ok(CourtSet {
            ladder: serde_json::from_str(s)?,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::from_json(&std::fs::read_to_string(path)?)?)
    }

    /// Load from a repo root: `<root>/reports/claim-ladder.json`. This is the consumption entry point
    /// for `kobold-courts check --root <repo>` (gnucobol-rs verifies its OWN evidence via this tool).
    pub fn load_root(root: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        Self::load(root.as_ref().join("reports").join("claim-ladder.json"))
    }

    /// Validate: unique ids, and negatives >= positives for every court.
    pub fn validate(&self) -> Vec<Violation> {
        let mut v = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for c in &self.ladder.courts {
            if !seen.insert(c.id.as_str()) {
                v.push(Violation {
                    court_id: c.id.clone(),
                    kind: "duplicate_id".into(),
                    detail: "court id is not unique".into(),
                });
            }
            if !c.negatives_ge_positives() {
                v.push(Violation {
                    court_id: c.id.clone(),
                    kind: "negatives_lt_positives".into(),
                    detail: format!(
                        "{} positives > {} negatives (negative capability is the trust surface)",
                        c.positive_claims().len(),
                        c.negative_claims().len()
                    ),
                });
            }
        }
        v
    }

    pub fn casefiles(&self) -> Vec<Casefile> {
        self.ladder.courts.iter().map(Casefile::from_court).collect()
    }

    pub fn court(&self, id: &str) -> Option<&Court> {
        self.ladder.courts.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semicolon_split_and_invariant() {
        let c = Court {
            proven: "did A; did B; did C".into(),
            not_proven: "no X; no Y".into(),
            lie_prevented: "Z is not implied".into(),
            ..Default::default()
        };
        assert_eq!(c.positive_claims().len(), 3);
        assert_eq!(c.negative_claims().len(), 3); // no X, no Y, lie prevented: Z
        assert!(c.negatives_ge_positives());
    }

    #[test]
    fn plus_split_only_before_uppercase_or_paren() {
        let c = Court {
            proven: "A thing + Behavior + (a paren) + lowercase stays joined".into(),
            ..Default::default()
        };
        assert_eq!(
            c.positive_claims(),
            vec!["A thing", "Behavior", "(a paren) + lowercase stays joined"]
        );
    }

    #[test]
    fn negatives_lt_positives_is_a_violation() {
        let c = Court {
            id: "X".into(),
            proven: "a; b; c".into(),
            not_proven: "only one".into(),
            ..Default::default()
        };
        assert!(!c.negatives_ge_positives());
    }

    #[test]
    fn load_validate_casefile_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample-claim-ladder.json");
        let cs = CourtSet::load(path).expect("load fixture");
        assert!(cs.ladder.courts.len() >= 2);
        assert!(cs.validate().is_empty(), "fixture must be clean: {:?}", cs.validate());
        assert_eq!(cs.casefiles().len(), cs.ladder.courts.len());
        assert_eq!(cs.court("GNURUST.EXAMPLE.1").map(|c| c.kind()), Some("court"));
    }

    #[test]
    fn frozen_gnucobol_rs_ladder_reproduces_the_verdict() {
        // Commit-1 proof: kobold-courts reproduces gnucobol-rs's court verdict on its FROZEN evidence,
        // exactly as the lab's casefile gate did (negatives >= positives, ids unique), before extraction.
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/frozen");
        let cs = CourtSet::load_root(root).expect("load frozen root");
        assert!(cs.ladder.courts.len() >= 90, "frozen ladder has the real court set");
        assert!(
            cs.validate().is_empty(),
            "frozen gnucobol-rs ladder must verify clean: {:?}",
            cs.validate()
        );
    }
}

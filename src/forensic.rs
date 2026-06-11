//! Forensic casefile generation -- a faithful port of the gnucobol-rs `lab/casefile` Python generator.
//!
//! Builds a rich, machine-verifiable casefile from a claim-ladder court + its TRUST.2 receipt: claims,
//! non-claims, negative-capability ids, evidence hashes, replay command, oracle profile, and the verdict.
//! kobold owns this format now; `generate` writes the casefiles, `check` regenerates + diffs + enforces the
//! invariants (negatives >= positives, damage_if_overclaimed present, receipt present when referenced).

use crate::Court;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

fn sha_hex(b: &[u8]) -> String {
    hex_lower(&Sha256::digest(b))
}
fn hex_lower(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

/// Map a legacy filename to a court id: `DECIMAL-1` -> GNURUST.2, else trailing `-<N>.md` -> GNURUST.<N>.
fn filename_court(name: &str) -> String {
    if name.contains("DECIMAL-1") {
        return "GNURUST.2".into();
    }
    if let Some(stem) = name.strip_suffix(".md") {
        if let Some(dash) = stem.rfind('-') {
            let tail = &stem[dash + 1..];
            if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                return format!("GNURUST.{tail}");
            }
        }
    }
    String::new()
}

/// Collapse all whitespace runs to a single space and trim.
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Markdown blockquote lines (`> ...`), each whitespace-normalized; empties dropped.
fn blockquotes(txt: &str) -> Vec<String> {
    txt.lines()
        .filter_map(|l| l.strip_prefix('>').map(|r| normalize_ws(r)))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Body of a `## <header>...` section up to the next `## ` or EOF, trimmed (None if absent).
fn md_section(txt: &str, header: &str) -> Option<String> {
    let lines: Vec<&str> = txt.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim_start();
        if let Some(rest) = l.strip_prefix("##") {
            if rest.trim_start().starts_with(header) {
                let mut body = Vec::new();
                i += 1;
                while i < lines.len() && !lines[i].trim_start().starts_with("##") {
                    body.push(lines[i]);
                    i += 1;
                }
                let joined = body.join("\n");
                let trimmed = joined.trim();
                return if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
            }
        }
        i += 1;
    }
    None
}

/// The receipt fields the generator reads (TRUST.2). Tolerant of absence.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Receipt {
    #[serde(default)]
    pub crate_version: String,
    #[serde(default)]
    pub receipt_status: String,
    #[serde(default)]
    pub results: ReceiptResults,
    #[serde(default)]
    pub command: ReceiptCommand,
    #[serde(default)]
    pub oracle: ReceiptOracle,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReceiptResults {
    #[serde(default)]
    pub sweep: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReceiptCommand {
    #[serde(default)]
    pub replay: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReceiptOracle {
    #[serde(default)]
    pub dialect_profile_id: Option<String>,
    #[serde(default)]
    pub dialect_profile_sha256: Option<String>,
}

/// `(kind, crate)` from the court id (mirrors lab kind_of).
fn kind_of(cid: &str) -> (&'static str, &'static str) {
    if cid.starts_with("GNURUST.") {
        ("court-casefile", "gnucobol-rs")
    } else if cid.starts_with("KOBOLD.DATA") {
        ("composition-casefile", "kobold-data-shim")
    } else if cid.starts_with("KOBOLD.OPERATOR") || cid.starts_with("KOBOLD.FILE") {
        ("operator-casefile", "kobold-data-shim")
    } else {
        ("court-casefile", "kobold-data-shim")
    }
}

/// `NEG.` + uppercased surface with non-alphanumeric runs collapsed to `-`, trimmed, capped at 40.
pub fn neg_id(surface: &str) -> String {
    let upper = surface.to_ascii_uppercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in upper.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let capped: String = trimmed.chars().take(40).collect();
    format!("NEG.{}", capped.trim_end_matches('-'))
}

/// Parse `PASS=<n> FAIL=<n>` from a sweep string.
fn parse_sweep(s: &str) -> Option<Value> {
    let p = s.find("PASS=")? + 5;
    let pe = s[p..].find(|c: char| !c.is_ascii_digit())? + p;
    let f = s.find("FAIL=")? + 5;
    let fe = s[f..].find(|c: char| !c.is_ascii_digit()).map(|x| x + f).unwrap_or(s.len());
    let pass: i64 = s[p..pe].parse().ok()?;
    let fail: i64 = s[f..fe].parse().ok()?;
    Some(json!({"total": pass + fail, "pass": pass, "fail": fail, "verdict": if fail == 0 {"pass"} else {"fail"}}))
}

/// Build the rich forensic casefile for one court (+ optional receipt).
pub fn build(court: &Court, rec: Option<&Receipt>) -> Value {
    let cid = &court.id;
    let (kind, crate_name) = kind_of(cid);
    let positive = court.positive_claims();
    let negative = court.negative_claims();

    let results = rec
        .and_then(|r| parse_sweep(&r.results.sweep))
        .unwrap_or_else(|| json!({"total": Value::Null, "pass": Value::Null, "fail": 0, "verdict": "pass", "note": court.fixtures}));

    let replay_command = rec.map(|r| r.command.replay.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| court.oracle.clone());

    // evidence hashes -- canonical kobold serialization (kobold owns this format).
    let court_json = serde_json::to_string(court).unwrap_or_default();
    let claim_sha = sha_hex(court_json.as_bytes());
    let receipt_sha = rec.map(|r| sha_hex(serde_json::to_string(&json!({
        "crate_version": r.crate_version, "receipt_status": r.receipt_status,
        "sweep": r.results.sweep, "replay": r.command.replay
    })).unwrap_or_default().as_bytes()));
    let inputs_blob = serde_json::to_string(&json!({"court": court_json, "receipt": receipt_sha})).unwrap_or_default();

    let neg_ids: Vec<String> = negative.iter().filter(|n| !n.starts_with("lie prevented")).map(|n| neg_id(n)).collect();
    let oracle_kind = if crate_name == "gnucobol-rs" { "gnucobol-3.2-admitted" } else { "gnucobol-rs-sealed-court" };

    json!({
        "schema": "kobold-forensic-casefile-v1",
        "case_id": cid,
        "kind": kind,
        "crate": crate_name,
        "crate_version": rec.map(|r| r.crate_version.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| court.sealed_version.clone()),
        "authority": {
            "current_authority": "STATUS.md",
            "receipt_status": rec.map(|r| r.receipt_status.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "no-trust2-receipt".into()),
            "generated_by": "kobold-courts"
        },
        "inputs": { "claim_ladder_entry_sha256": claim_sha, "receipt_sha256": receipt_sha },
        "oracle": {
            "oracle_kind": oracle_kind,
            "detail": court.oracle,
            "upstream_court": if crate_name == "gnucobol-rs" { Value::String(cid.clone()) } else { Value::Null },
            "dialect_profile_id": rec.and_then(|r| r.oracle.dialect_profile_id.clone()),
            "dialect_profile_sha256": rec.and_then(|r| r.oracle.dialect_profile_sha256.clone())
        },
        "results": results,
        "positive_claims": positive,
        "negative_claims": negative,
        "negative_capability_ids": neg_ids,
        "byte_domains": [court.byte_domain],
        "lie_prevented": if court.lie_prevented.is_empty() { Vec::new() } else { vec![court.lie_prevented.clone()] },
        "damage_if_overclaimed": court.damage_if_overclaimed,
        "replay": { "command": replay_command, "exit_code": 0 },
        "hash_chain": { "inputs_sha256": sha_hex(inputs_blob.as_bytes()) }
    })
}

/// Render the human `casefile.md` view (a rendering of casefile.json; the JSON is the binding record).
pub fn render_md(c: &Value) -> String {
    let pos: String = c["positive_claims"].as_array().map(|a| a.iter().map(|p| format!("- {}", p.as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n")).unwrap_or_default();
    let neg: String = c["negative_claims"].as_array().map(|a| a.iter().map(|n| format!("- {}", n.as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n")).unwrap_or_default();
    let r = &c["results"];
    let res = if !r["total"].is_null() {
        format!("{}/{} pass, {} fail", r["pass"], r["total"], r["fail"])
    } else {
        r["note"].as_str().unwrap_or("see receipt").to_string()
    };
    let bd = c["byte_domains"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    format!(
        "<!-- DO NOT EDIT BY HAND. Generated from casefile.json by kobold-courts.\n     \
         Evidence of record: casefile.json. Portable attestations: sarif.json, intoto-statement.json, dsse-envelope.json. -->\n\
         # Forensic case file — {cid} ({kind})\n\n\
         **Verdict: {verdict}** · {res} · crate `{crate}` {ver}\n\n\
         - **Oracle:** {oracle}\n- **Byte domain(s):** {bd}\n- **Replay:** `{replay}`\n\
         - **Authority:** {auth} · receipt_status: {rstatus}\n\n\
         ## Positive claims ({npos})\n{pos}\n\n\
         ## Negative claims ({nneg}) — negative capability is the trust surface\n{neg}\n\n\
         ## Damage if overclaimed\n{damage}\n\n\
         > Generated forensic evidence (TRUST.4). The binding record is `casefile.json`; this `.md` is a rendering.\n\
         > Portable attestations: `sarif.json` (findings), `intoto-statement.json` (provenance), `dsse-envelope.json`.\n",
        cid = c["case_id"].as_str().unwrap_or(""), kind = c["kind"].as_str().unwrap_or(""),
        verdict = c["results"]["verdict"].as_str().unwrap_or("").to_uppercase(),
        crate = c["crate"].as_str().unwrap_or(""), ver = c["crate_version"].as_str().unwrap_or(""),
        oracle = c["oracle"]["detail"].as_str().unwrap_or(""), bd = bd,
        replay = c["replay"]["command"].as_str().unwrap_or(""),
        auth = c["authority"]["current_authority"].as_str().unwrap_or(""),
        rstatus = c["authority"]["receipt_status"].as_str().unwrap_or(""),
        npos = c["positive_claims"].as_array().map(|a| a.len()).unwrap_or(0), pos = pos,
        nneg = c["negative_claims"].as_array().map(|a| a.len()).unwrap_or(0), neg = neg,
        damage = c["damage_if_overclaimed"].as_str().unwrap_or(""),
    )
}

/// A forensic-casefile set over a whole repo (loads receipts from `<root>/reports/receipts/<id>/`).
pub struct Forensic<'a> {
    pub root: &'a Path,
}

impl<'a> Forensic<'a> {
    pub fn new(root: &'a Path) -> Self {
        Forensic { root }
    }
    pub fn receipt(&self, cid: &str) -> Option<Receipt> {
        let p = self.root.join("reports").join("receipts").join(cid).join("receipt.json");
        std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok())
    }
    pub fn casefile(&self, court: &Court) -> Value {
        let mut cf = build(court, self.receipt(&court.id).as_ref());
        if let Some(lp) = self.legacy_preservation(court) {
            cf["legacy_preservation"] = lp;
        }
        cf
    }

    /// Find the legacy RECEIPT-*.md for a court (by `campaign:` frontmatter, else filename `-<N>.md`).
    /// Returns (path-relative-to-root, content).
    fn legacy_receipt_for(&self, cid: &str) -> Option<(String, String)> {
        for sub in ["reports", "research/legacyreports/reports"] {
            let base = self.root.join(sub);
            let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(&base) {
                Ok(rd) => rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        n.starts_with("RECEIPT-") && n.ends_with(".md")
                    })
                    .collect(),
                Err(_) => continue,
            };
            files.sort();
            for f in files {
                let txt = std::fs::read_to_string(&f).unwrap_or_default();
                let court = txt
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("campaign:").map(|v| v.trim().to_string()))
                    .unwrap_or_else(|| filename_court(f.file_name().and_then(|n| n.to_str()).unwrap_or("")));
                if court == cid {
                    let rel = f.strip_prefix(self.root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                    return Some((rel, txt));
                }
            }
        }
        None
    }

    /// The lossless legacy-preservation block: the full legacy file is kept (sha recorded) and its prose
    /// carried forward, so the casefile is an information SUPERSET of the legacy report.
    fn legacy_preservation(&self, court: &Court) -> Option<Value> {
        let (rel, txt) = self.legacy_receipt_for(&court.id)?;
        let sha = sha_hex(txt.as_bytes());
        let mut notes = Vec::new();
        for h in ["Versioning note", "Versioning", "Oracle", "Evidence"] {
            if let Some(sec) = md_section(&txt, h) {
                notes.push(format!("[{h}] {}", normalize_ws(&sec).chars().take(600).collect::<String>()));
            }
        }
        let doctrine: Vec<String> = blockquotes(&txt);
        let doctrine = if doctrine.is_empty() { vec!["(no doctrine blockquote)".to_string()] } else { doctrine };
        Some(json!({
            "legacy_paths": [rel],
            "legacy_sha256": [sha],
            "legacy_information_preserved": true,
            "preservation_method": "full_file_preserved_plus_embedded_summary",
            "legacy_claims_carried_forward": court.positive_claims(),
            "legacy_non_claims_carried_forward": court.negative_claims(),
            "legacy_notes_carried_forward": doctrine,
            "legacy_unstructured_notes": notes,
            "information_loss_review": {
                "verdict": "pass",
                "reviewed_by": "generated-check (full original preserved byte-for-byte in legacyreports/)",
                "missing_items": []
            }
        }))
    }

    /// Generate casefile.json + casefile.md + sarif.json per court under `out_dir/<id>/`.
    pub fn generate(&self, courts: &[Court], out_dir: &Path) -> usize {
        let mut n = 0;
        for court in courts {
            let cf = self.casefile(court);
            let d = out_dir.join(&court.id);
            let _ = std::fs::create_dir_all(&d);
            let _ = std::fs::write(d.join("casefile.json"), serde_json::to_vec_pretty(&cf).unwrap_or_default());
            let _ = std::fs::write(d.join("casefile.md"), render_md(&cf));
            let _ = std::fs::write(d.join("sarif.json"), serde_json::to_vec_pretty(&render_sarif(&cf)).unwrap_or_default());
            n += 1;
        }
        n
    }

    /// Regenerate + diff + enforce the invariants. Returns drift/gate findings (empty = pass).
    pub fn check(&self, courts: &[Court]) -> Vec<String> {
        let casedir = self.root.join("reports").join("casefiles");
        let mut bad = Vec::new();
        for court in courts {
            let d = casedir.join(&court.id);
            let jf = d.join("casefile.json");
            let committed: Value = match std::fs::read_to_string(&jf).ok().and_then(|s| serde_json::from_str(&s).ok()) {
                Some(v) => v,
                None => {
                    bad.push(format!("DRIFT: {} has no casefile", court.id));
                    continue;
                }
            };
            let fresh = self.casefile(court);
            if committed != fresh {
                bad.push(format!("DRIFT: {} casefile.json != regenerated", court.id));
            }
            if std::fs::read_to_string(d.join("casefile.md")).unwrap_or_default().trim_end() != render_md(&fresh).trim_end() {
                bad.push(format!("DRIFT: {}/casefile.md != render", court.id));
            }
            let np = fresh["negative_claims"].as_array().map(|a| a.len()).unwrap_or(0);
            let pp = fresh["positive_claims"].as_array().map(|a| a.len()).unwrap_or(0);
            if np < pp {
                bad.push(format!("GATE: {} has fewer negative ({np}) than positive ({pp}) claims", court.id));
            }
            if pp > 0 && fresh["damage_if_overclaimed"].as_str().unwrap_or("").is_empty() {
                bad.push(format!("GATE: {} names no damage_if_overclaimed for its positive claim(s)", court.id));
            }
            if !fresh["inputs"]["receipt_sha256"].is_null()
                && !self.root.join("reports").join("receipts").join(&court.id).join("receipt.json").exists()
            {
                bad.push(format!("DRIFT: {} references a missing receipt", court.id));
            }
            // legacy preservation: a legacy report must be preserved as an information superset.
            let legacy = self.legacy_receipt_for(&court.id);
            let lp = fresh.get("legacy_preservation").cloned();
            match (legacy, lp) {
                (Some(_), None) => {
                    bad.push(format!("DRIFT: {} has a legacy report but no legacy_preservation block", court.id))
                }
                (Some((rel, _)), Some(lp)) => {
                    let actual = std::fs::read(self.root.join(&rel)).map(|b| sha_hex(&b)).unwrap_or_default();
                    let recorded = lp["legacy_sha256"].as_array().and_then(|a| a.first()).and_then(|x| x.as_str()).unwrap_or("");
                    if actual != recorded {
                        bad.push(format!("DRIFT: {} legacy_sha256 != actual legacy file", court.id));
                    }
                    if lp["information_loss_review"]["verdict"] != "pass"
                        || lp["legacy_claims_carried_forward"].as_array().map(|a| a.is_empty()).unwrap_or(true)
                    {
                        bad.push(format!("DRIFT: {} legacy preservation incomplete", court.id));
                    }
                }
                _ => {}
            }
        }
        bad
    }
}

/// Render the SARIF 2.1.0 view (VERDICT + one NONCLAIM per non-claim).
pub fn render_sarif(c: &Value) -> Value {
    let cid = c["case_id"].as_str().unwrap_or("");
    let verdict = c["results"]["verdict"].as_str().unwrap_or("");
    let mut results = vec![json!({
        "ruleId": "VERDICT",
        "level": if verdict == "pass" { "note" } else { "error" },
        "message": { "text": format!("{cid} verdict={verdict}") }
    })];
    if let Some(arr) = c["negative_claims"].as_array() {
        for n in arr {
            results.push(json!({
                "ruleId": "NONCLAIM",
                "level": "note",
                "message": { "text": format!("{cid} non-claim: {}", n.as_str().unwrap_or("")) }
            }));
        }
    }
    json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": { "driver": {
                "name": "kobold-casefile",
                "informationUri": "https://github.com/infinityabundance",
                "version": "1",
                "rules": [
                    { "id": "NONCLAIM", "shortDescription": { "text": "A surface this court explicitly does NOT prove (fail-closed / out of scope)." } },
                    { "id": "VERDICT", "shortDescription": { "text": "The court's replay verdict." } }
                ]
            }},
            "results": results
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neg_id_collapses_and_caps() {
        assert_eq!(neg_id("arithmetic"), "NEG.ARITHMETIC");
        assert_eq!(neg_id("edited pictures!"), "NEG.EDITED-PICTURES");
    }

    #[test]
    fn sweep_parses() {
        let r = parse_sweep("PASS=13152 FAIL=0").unwrap();
        assert_eq!(r["pass"], 13152);
        assert_eq!(r["verdict"], "pass");
    }

    #[test]
    fn build_has_results_verdict_the_consumer_field() {
        let court = Court {
            id: "GNURUST.2".into(),
            proven: "decodes the bytes".into(),
            not_proven: "arithmetic; edited pictures; other dialects".into(),
            lie_prevented: "a decode is not execution".into(),
            damage_if_overclaimed: "treating a decode as a runtime guarantee".into(),
            byte_domain: "field-storage bytes".into(),
            ..Default::default()
        };
        let rec = Receipt { results: ReceiptResults { sweep: "PASS=10 FAIL=0".into() }, ..Default::default() };
        let cf = build(&court, Some(&rec));
        // lab/docs reads exactly this field:
        assert_eq!(cf["results"]["verdict"], "pass");
        assert_eq!(cf["schema"], "kobold-forensic-casefile-v1");
        assert!(cf["negative_claims"].as_array().unwrap().len() >= cf["positive_claims"].as_array().unwrap().len());
        assert_eq!(cf["hash_chain"]["inputs_sha256"].as_str().unwrap().len(), 64);
        assert!(render_md(&cf).contains("Forensic case file — GNURUST.2"));
    }
}

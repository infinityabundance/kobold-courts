//! kobold-courts CLI: validate a claim-ladder, summarize it, or emit casefiles.
//!   kobold-courts check --root <repo>            (verify <repo>/reports/claim-ladder.json)
//!   kobold-courts validate  <claim-ladder.json>
//!   kobold-courts summary   <claim-ladder.json>
//!   kobold-courts casefiles <claim-ladder.json>   (one JSON casefile per line)
use kobold_courts::CourtSet;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage =
        "usage: kobold-courts <check --root <repo> | validate|summary|casefiles <claim-ladder.json>>";
    if args.len() < 3 {
        eprintln!("{usage}");
        exit(2);
    }
    // check --root <repo>: the gnucobol-rs consumption entry point.
    if args[1] == "check" && args[2] == "--root" {
        let root = args.get(3).map(String::as_str).unwrap_or(".");
        let cs = match CourtSet::load_root(root) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("load error: {e}");
                exit(2);
            }
        };
        let v = cs.validate();
        if v.is_empty() {
            println!(
                "kobold-courts: PASS  {} courts  negatives>=positives  ids unique  (root {root})",
                cs.ladder.courts.len()
            );
        } else {
            for x in &v {
                println!("VIOLATION {} [{}] {}", x.court_id, x.kind, x.detail);
            }
            println!("kobold-courts: FAIL  {} violation(s)", v.len());
            exit(1);
        }
        return;
    }
    let cs = match CourtSet::load(&args[2]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("load error: {e}");
            exit(2);
        }
    };
    match args[1].as_str() {
        "validate" => {
            let v = cs.validate();
            if v.is_empty() {
                println!(
                    "PASS: {} courts, negatives>=positives, ids unique",
                    cs.ladder.courts.len()
                );
            } else {
                for x in &v {
                    println!("VIOLATION {} [{}] {}", x.court_id, x.kind, x.detail);
                }
                println!("FAIL: {} violation(s)", v.len());
                exit(1);
            }
        }
        "summary" => {
            println!("{} courts (schema={})", cs.ladder.courts.len(), cs.ladder.schema);
            for c in &cs.ladder.courts {
                println!(
                    "  {:<40} readiness={} +{}/-{} [{}]",
                    c.id,
                    c.readiness,
                    c.positive_claims().len(),
                    c.negative_claims().len(),
                    c.kind()
                );
            }
        }
        "casefiles" => {
            for cf in cs.casefiles() {
                match serde_json::to_string(&cf) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("serialize error: {e}");
                        exit(2);
                    }
                }
            }
        }
        _ => {
            eprintln!("{usage}");
            exit(2);
        }
    }
}

//! kobold-courts CLI: validate a claim-ladder, summarize it, or emit casefiles.
//!   kobold-courts check --root <repo>            (verify <repo>/reports/claim-ladder.json)
//!   kobold-courts validate  <claim-ladder.json>
//!   kobold-courts summary   <claim-ladder.json>
//!   kobold-courts casefiles <claim-ladder.json>   (one JSON casefile per line)
use kobold_courts::forensic::Forensic;
use kobold_courts::CourtSet;
use std::path::Path;
use std::process::exit;

fn arg_after(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage =
        "usage: kobold-courts <check --root <repo> | casefile generate|check --root <repo> | validate|summary|casefiles <file>>";
    if args.len() < 2 {
        eprintln!("{usage}");
        exit(2);
    }
    // casefile generate|check --root <repo>: the forensic-casefile generator/gate (lab/casefile successor).
    if args[1] == "casefile" {
        let sub = args.get(2).map(String::as_str).unwrap_or("");
        let root = arg_after(&args, "--root").unwrap_or_else(|| ".".into());
        let cs = match CourtSet::load_root(&root) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("load error: {e}");
                exit(2);
            }
        };
        let f = Forensic::new(Path::new(&root));
        match sub {
            "generate" => {
                let out = arg_after(&args, "--out").unwrap_or_else(|| format!("{root}/reports/casefiles"));
                let n = f.generate(&cs.ladder.courts, Path::new(&out));
                println!("generated {n} forensic case files in {out}");
            }
            "check" => {
                let bad = f.check(&cs.ladder.courts);
                if bad.is_empty() {
                    println!(
                        "kobold-courts casefile: PASS ({} courts; generated views match; negatives>=positives; receipts present)",
                        cs.ladder.courts.len()
                    );
                } else {
                    for b in &bad {
                        println!("{b}");
                    }
                    println!("!! {} casefile finding(s)", bad.len());
                    exit(1);
                }
            }
            _ => {
                eprintln!("usage: kobold-courts casefile <generate|check> --root <repo> [--out <dir>]");
                exit(2);
            }
        }
        return;
    }
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

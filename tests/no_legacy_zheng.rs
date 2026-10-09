//! Guard: foculus src reaches zheng only through the sound surface.
//!
//! zheng's legacy commit / open / verify / fold / decide path is unsound
//! (the fold is never checked; zheng specs/decider.md). Every `zheng::…`
//! path in src must be on the allow-list below; anything else — a legacy
//! function, `Accumulator`, `Transcript`, `folding`, `zheng::Proof` — fails.

use std::path::Path;

const ALLOWED: &[&str] = &[
    "zheng::execution::Certificate",
    "zheng::execution::certificate",
    "zheng::types::CCSInstance",
    "zheng::types::CCSWitness",
    "zheng::types::SparseMatrix",
];

/// Test-only exceptions (file, path). None: the tri-kernel port is pinned to
/// frozen zheng 0.4 vectors, not to zheng's legacy `phi`.
const TEST_ONLY: &[(&str, &str)] = &[];

fn ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Every full `zheng::…` path in `text`, with `{a, b}` groups expanded.
fn paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("zheng::") {
        let before = rest[..i].chars().last();
        rest = &rest[i..];
        if before.is_some_and(ident) {
            rest = &rest[7..];
            continue;
        }
        let end = rest
            .find(|c: char| !(ident(c) || c == ':'))
            .unwrap_or(rest.len());
        let head = rest[..end].trim_end_matches(':').to_string();
        if rest[end..].starts_with('{') {
            let close = rest[end..].find('}').expect("closed group") + end;
            for item in rest[end + 1..close].split(',') {
                let item = item.trim();
                if !item.is_empty() {
                    out.push(format!("{head}::{}", item.split_whitespace().next().unwrap()));
                }
            }
            rest = &rest[close..];
        } else {
            out.push(head);
            rest = &rest[end..];
        }
    }
    out
}

fn walk(dir: &Path, found: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            for p in paths(&text) {
                found.push((name.clone(), p));
            }
        }
    }
}

#[test]
fn src_uses_no_legacy_zheng_api() {
    let mut found = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
    assert!(!found.is_empty(), "scanner found no zheng paths at all");
    let bad: Vec<_> = found
        .iter()
        .filter(|(file, p)| {
            !ALLOWED.iter().any(|a| p == a || p.starts_with(&format!("{a}::")))
                && !TEST_ONLY.iter().any(|(f, a)| file == f && p == a)
        })
        .collect();
    assert!(bad.is_empty(), "legacy zheng API in foculus src: {bad:?}");
}

#[test]
fn scanner_catches_legacy_paths() {
    let legacy = "use zheng::{Transcript, decide, fold};\nlet p: zheng::Proof;\nzheng::phi::verify_phi_star(..)";
    let ps = paths(legacy);
    for want in ["zheng::Transcript", "zheng::decide", "zheng::fold", "zheng::Proof", "zheng::phi::verify_phi_star"] {
        assert!(ps.iter().any(|p| p == want), "missed {want} in {ps:?}");
    }
    assert!(paths("cyberzheng::x").is_empty());
}

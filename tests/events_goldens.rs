//! SPEC §11.3 item 3 (Class A): `bc events` byte-equals all four
//! `goldens/events/*.events.jsonl` files.

use bc::{events, jsonl, score};

fn check(name: &str, want_events: usize) {
    let root = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{root}/examples/{name}.bc")).expect("read score");
    let want = std::fs::read_to_string(format!("{root}/goldens/events/{name}.events.jsonl"))
        .expect("read golden");
    let sc = score::parse(&src).expect("parse");
    let evs = events::compile(&sc).expect("compile");
    assert_eq!(evs.len(), want_events, "{name}: event count");
    let got = jsonl::events_jsonl(&evs);
    if got != want {
        for (i, (g, w)) in got.lines().zip(want.lines()).enumerate() {
            assert_eq!(g, w, "{name}: first divergence at event line {}", i + 1);
        }
        panic!("{name}: line count differs: got {}", got.lines().count());
    }
}

#[test]
fn four_events_byte_exact() {
    check("four", 24);
}

#[test]
fn dilla_events_byte_exact() {
    check("dilla", 44);
}

#[test]
fn poly_events_byte_exact() {
    check("poly", 86);
}

#[test]
fn edge_events_byte_exact() {
    check("edge", 58);
}

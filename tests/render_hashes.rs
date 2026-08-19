//! SPEC §11.3 item 7 (Class C): this implementation's render hashes
//! for the four example scores, committed to
//! `goldens/renders-v0.1.txt`. Running this test on the CI OS matrix
//! is the cross-machine equality proof; running it twice anywhere is
//! covered by the double-render test.

use bc::{events, render, score};

#[test]
fn render_hashes_match_committed_goldens() {
    let root = env!("CARGO_MANIFEST_DIR");
    let committed = std::fs::read_to_string(format!("{root}/goldens/renders-v0.1.txt"))
        .expect("read renders-v0.1.txt");
    let mut checked = 0;
    for line in committed.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let name = it.next().expect("name");
        let want = it.next().expect("hash");
        let stem = name.strip_suffix(".wav").expect(".wav name");
        let src =
            std::fs::read_to_string(format!("{root}/examples/{stem}.bc")).expect("read score");
        let sc = score::parse(&src).expect("parse");
        let evs = events::compile(&sc).expect("compile");
        let r = render::render(&evs).expect("render");
        assert_eq!(r.sha256_hex, want, "{name}: render hash");
        checked += 1;
    }
    assert_eq!(checked, 4, "all four example scores committed");
}

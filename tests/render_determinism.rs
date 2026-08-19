//! PLAN.md Phase 2 gate: double-render byte equality (the same-machine
//! half of SPEC §11.3 item 7 — CI's OS matrix against
//! goldens/renders-v0.1.txt is the cross-machine half), plus render
//! facts the spec pins.

use bc::{events, render, score};

fn render_example(name: &str) -> render::Render {
    let src = std::fs::read_to_string(format!("{}/examples/{name}.bc", env!("CARGO_MANIFEST_DIR")))
        .expect("read score");
    let sc = score::parse(&src).expect("parse");
    let evs = events::compile(&sc).expect("compile");
    render::render(&evs).expect("render")
}

/// Two renders of every example, byte-identical WAVs and equal hashes.
#[test]
fn double_render_byte_equality() {
    for name in ["four", "dilla", "poly", "edge"] {
        let a = render_example(name);
        let b = render_example(name);
        assert_eq!(a.wav_bytes, b.wav_bytes, "{name}: WAV bytes");
        assert_eq!(a.sha256_hex, b.sha256_hex, "{name}: sha256");
    }
}

/// Empty mix ⇒ exactly one second of silence (SPEC §9.4).
#[test]
fn empty_mix_is_one_second() {
    let sc = score::parse("tempo 100\n").expect("parse");
    let evs = events::compile(&sc).expect("compile");
    assert!(evs.is_empty());
    let r = render::render(&evs).expect("render");
    assert_eq!(r.frames, 44100);
    assert!(!r.peak_normalized);
    assert!(r.wav_bytes[44..].iter().all(|&b| b == 0), "silence");
}

/// Of the committed scores, only edge.bc trips peak normalization
/// (by design — gain=1.1; SPEC §9.5).
#[test]
fn peak_normalization_only_on_edge() {
    for (name, want) in [
        ("four", false),
        ("dilla", false),
        ("poly", false),
        ("edge", true),
    ] {
        assert_eq!(
            render_example(name).peak_normalized,
            want,
            "{name}: peak-normalized flag"
        );
    }
}

/// s16 range is symmetric ±32767 — the −32768 code is never produced
/// (SPEC §9.6).
#[test]
fn s16_range_symmetric() {
    for name in ["four", "dilla", "poly", "edge"] {
        let r = render_example(name);
        for chunk in r.wav_bytes[44..].chunks_exact(2) {
            let v = i16::from_le_bytes(chunk.try_into().expect("2"));
            assert!(v != i16::MIN, "{name}: -32768 must never appear");
        }
    }
}

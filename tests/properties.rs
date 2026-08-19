//! The six SPEC §11.1 property tests (§11.3 item 5).

use bc::{events, jsonl, render, score};

fn compile_jsonl(src: &str) -> String {
    let sc = score::parse(src).expect("parse");
    jsonl::events_jsonl(&events::compile(&sc).expect("compile"))
}

fn compile_events(src: &str) -> Vec<events::Event> {
    events::compile(&score::parse(src).expect("parse")).expect("compile")
}

fn read_example(name: &str) -> String {
    std::fs::read_to_string(format!("{}/examples/{name}.bc", env!("CARGO_MANIFEST_DIR")))
        .expect("read example")
}

/// 1 — compile determinism: two compiles byte-equal.
#[test]
fn p1_compile_determinism() {
    for name in ["four", "dilla", "poly", "edge"] {
        let src = read_example(name);
        assert_eq!(compile_jsonl(&src), compile_jsonl(&src), "{name}");
    }
}

/// 2 — render determinism: two renders, equal sha256.
#[test]
fn p2_render_determinism() {
    let src = read_example("dilla");
    let evs = compile_events(&src);
    assert_eq!(
        render::render(&evs).sha256_hex,
        render::render(&evs).sha256_hex
    );
}

/// 3 — swing 50% is the identity on performed times.
#[test]
fn p3_swing_50_identity() {
    let base = "tempo 97\nseed 5\nvoice k sample=kick\n  gate xXx.xxX.\n";
    let swung = "tempo 97\nseed 5\nswing 50% 1/16\nvoice k sample=kick\n  gate xXx.xxX.\n";
    assert_eq!(compile_jsonl(base), compile_jsonl(swung));
}

/// 4 — swing 66% delays off-sixteenths and leaves downbeats untouched.
#[test]
fn p4_swing_66_delays_offbeats() {
    let base = "tempo 120\nvoice k sample=kick\n  gate xxxxxxxxxxxxxxxx\n";
    let swung = "tempo 120\nswing 66% 1/16\nvoice k sample=kick\n  gate xxxxxxxxxxxxxxxx\n";
    let a = compile_events(base);
    let b = compile_events(swung);
    assert_eq!(a.len(), 16);
    assert_eq!(b.len(), 16);
    for (ea, eb) in a.iter().zip(&b) {
        assert_eq!(ea.step, eb.step);
        if ea.step % 2 == 1 {
            // Odd sixteenths (odd multiples of the 1/4-beat sub) delayed.
            assert!(
                eb.performed_s > ea.performed_s,
                "step {}: {} !> {}",
                ea.step,
                eb.performed_s,
                ea.performed_s
            );
            assert!(eb.swing_ms > 0.0);
        } else {
            // Downbeats and even sixteenths untouched.
            assert_eq!(ea.performed_s.to_bits(), eb.performed_s.to_bits());
            assert_eq!(eb.swing_ms.to_bits(), 0.0f64.to_bits());
        }
    }
}

/// 5 — humanize stable under the same seed, different under a new one.
#[test]
fn p5_humanize_seed_stability() {
    let with_seed = |seed: i64| {
        format!("tempo 110\nseed {seed}\nvoice h sample=hat\n  gate xxxxxxxx\n  hum 5\n")
    };
    let a1 = compile_jsonl(&with_seed(1));
    let a2 = compile_jsonl(&with_seed(1));
    let b = compile_jsonl(&with_seed(2));
    assert_eq!(a1, a2, "same seed ⇒ same jitter");
    assert_ne!(a1, b, "new seed ⇒ new (reproducible) performance");
}

/// 6 — WAV header sanity (RIFF/WAVE bytes, §9.7 layout).
#[test]
fn p6_wav_header_sanity() {
    let evs = compile_events(&read_example("four"));
    let r = render::render(&evs);
    let b = &r.wav_bytes;
    assert_eq!(&b[0..4], b"RIFF");
    assert_eq!(&b[8..12], b"WAVE");
    assert_eq!(&b[12..16], b"fmt ");
    assert_eq!(u16::from_le_bytes(b[20..22].try_into().expect("2")), 1);
    assert_eq!(u16::from_le_bytes(b[22..24].try_into().expect("2")), 2);
    assert_eq!(u32::from_le_bytes(b[24..28].try_into().expect("4")), 44100);
    assert_eq!(&b[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes(b[40..44].try_into().expect("4")) as usize,
        b.len() - 44
    );
}

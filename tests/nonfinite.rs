//! Non-finite hardening (audit findings; SPEC-GAPS #7): the oracle's
//! floats have no inf/NaN — overflow raises — so grammar-legal inputs
//! that would produce non-finite values are rejected cleanly instead
//! of leaking `inf`/`NaN` into events, JSONL, or the §12.5 rounder.

use bc::decfmt::{format_dec, round_dec};
use bc::error::Error;
use bc::{events, score};

fn run(src: &str) -> Result<Vec<events::Event>, Error> {
    events::compile(&score::parse(src)?)
}

/// Overflowing literals are bad numbers at parse, with the line cited.
#[test]
fn overflowing_literals_rejected_at_parse() {
    for (src, line) in [
        ("tempo 1e999\nvoice k sample=kick\n  gate x\n", 1),
        ("voice k sample=kick\n  gate x\n  hum 1e999\n", 3),
        ("voice k sample=kick\n  gate x\n  prob 1e999\n", 3),
        ("voice k sample=kick gain=1e999\n  gate x\n", 1),
        ("voice k sample=kick pan=-1e999\n  gate x\n", 1),
        ("voice k sample=kick\n  gate x\n  time [1e999]\n", 3),
        (
            "voice k sample=kick\n  gate x\n  vel [100] div=1/1\n  time [1e400ms]\n",
            4,
        ),
    ] {
        match run(src) {
            Err(Error::Score { line: l, .. }) => assert_eq!(l, line, "cited line for {src:?}"),
            other => panic!("{src:?}: expected line-cited rejection, got {other:?}"),
        }
    }
    // Underflow-to-subnormal literals still parse (they are finite).
    assert!(run("voice k sample=kick\n  gate x\n  hum 5e-324\n").is_ok());
}

/// A subnormal tempo would overflow spb to inf; clean compile error,
/// like the tempo-0 posture (SPEC §5.10 #11).
#[test]
fn subnormal_tempo_rejected_at_compile() {
    match run("tempo 5e-324\nvoice k sample=kick\n  gate x\n") {
        Err(Error::Compile(msg)) => assert!(msg.contains("tempo"), "{msg}"),
        other => panic!("expected compile error, got {other:?}"),
    }
}

/// Finite literals whose combination overflows mid-pipeline get a
/// clean compile error before anything non-finite reaches JSONL.
#[test]
fn midpipeline_overflow_rejected_at_compile() {
    // spb ≈ 6e301 (finite); step_s ≈ 1.5e301; Frac(1e308) × step_s = inf.
    let src = "tempo 1e-300\nvoice k sample=kick\n  gate x\n  time [1e308]\n";
    match run(src) {
        Err(Error::Compile(msg)) => assert!(msg.contains("non-finite"), "{msg}"),
        other => panic!("expected compile error, got {other:?}"),
    }
}

/// The §12.5 rounder itself never panics or wraps on non-finite input
/// (defense in depth — unreachable from the pipeline).
#[test]
fn decfmt_nonfinite_guard() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = round_dec(x, 6);
        assert_eq!(r.to_bits(), x.to_bits(), "round_dec keeps {x}");
        let _ = format_dec(x, 6); // must not panic
    }
}

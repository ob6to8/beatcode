//! Coverage the adversarial audit flagged as missing: stable-sort tie
//! order for duplicate voices (§2.3/§6.11), JSONL string escaping
//! (§7), the clean rational-overflow error (§3), notefrac sign
//! semantics (§5.7/§5.10 #12), the ms-scale overflow guard, and the
//! WAV-container render bound (SPEC-GAPS #9).

use bc::error::Error;
use bc::{events, jsonl, render, score};

fn compile(src: &str) -> Result<Vec<events::Event>, Error> {
    events::compile(&score::parse(src)?)
}

/// Ties beyond (performed_s, voice, step) — reachable only with
/// duplicate voice names — keep score order: the sort must be stable
/// (SPEC §2.3, §12.4 trap 6).
#[test]
fn duplicate_voice_ties_keep_score_order() {
    let src = "seed 1\n\
               voice k sample=kick\n  gate x...\n  vel [80]\n\
               voice k sample=kick\n  gate x...\n  vel [90]\n";
    let evs = compile(src).expect("compile");
    assert_eq!(evs.len(), 8);
    for pair in evs.chunks(2) {
        assert_eq!(pair[0].performed_s.to_bits(), pair[1].performed_s.to_bits());
        assert_eq!(pair[0].step, pair[1].step);
        assert_eq!(
            (pair[0].vel, pair[1].vel),
            (80, 90),
            "score order preserved"
        );
    }
}

/// §7 string escaping: only backslash and double quote, backslash
/// first; raw UTF-8 passes through unescaped. Voice names are
/// arbitrary tokens, so both specials are user-reachable.
#[test]
fn jsonl_escapes_backslash_and_quote_only() {
    let src = "voice a\"b\\cé sample=kick\n  gate x\nbars 1\ntempo 240\n";
    let evs = compile(src).expect("compile");
    let line = jsonl::event_line(&evs[0]);
    assert!(
        line.ends_with(",\"voice\":\"a\\\"b\\\\cé\"}"),
        "escaping: {line}"
    );
}

/// §3: checked rational arithmetic surfaces the clean "rational
/// overflow" error — at parse (line-cited, via 4n/d) and at compile
/// (pattern = 4 × bars).
#[test]
fn rational_overflow_clean_errors() {
    let at_parse = compile("voice k sample=kick\n  clock 4611686018427387904/1\n  gate x\n");
    match at_parse {
        Err(Error::Score { line, msg }) => {
            assert_eq!(line, 2);
            assert_eq!(msg, "rational overflow");
        }
        other => panic!("expected line-cited rational overflow, got {other:?}"),
    }
    let at_compile = compile("bars 4611686018427387903\nvoice k sample=kick\n  gate x\n");
    match at_compile {
        Err(Error::Compile(msg)) => assert_eq!(msg, "rational overflow"),
        other => panic!("expected compile rational overflow, got {other:?}"),
    }
}

/// §5.7/§5.10 #12: the notefrac's VALUE decides — `-1/-4` is a
/// positive quarter (accepted); zero or negative values reject.
#[test]
fn notefrac_sign_is_value_based() {
    assert!(compile("voice k sample=kick\n  clock -1/-4\n  gate x...\n").is_ok());
    for bad in ["0/4", "4/0", "-1/4", "1/-4"] {
        let src = format!("voice k sample=kick\n  clock {bad}\n  gate x\n");
        assert!(
            matches!(compile(&src), Err(Error::Score { line: 2, .. })),
            "clock {bad} must reject at line 2"
        );
    }
}

/// The finiteness guard runs on the STORED ms-scale fields: a finite
/// seconds offset whose ×1000 overflows must reject, never emit `inf`
/// (audit finding; SPEC-GAPS #7).
#[test]
fn ms_scale_overflow_rejected() {
    let src = "tempo 120\nvoice k sample=kick\n  gate x\n  time [4e306]\n";
    match compile(src) {
        Err(Error::Compile(msg)) => assert!(msg.contains("non-finite"), "{msg}"),
        other => panic!("expected compile rejection, got {other:?}"),
    }
}

/// Huge-but-finite placements exceed the WAV container's u32 sizes:
/// clean render error, not a panic/abort (SPEC-GAPS #9). The event
/// compile itself still succeeds (Class A is unaffected).
#[test]
fn render_too_long_is_clean_error() {
    let src = "tempo 120\nvoice k sample=kick\n  gate x\n  time [1e18ms]\n";
    let evs = compile(src).expect("events compile fine");
    match render::render(&evs) {
        Err(Error::Compile(msg)) => assert!(msg.contains("WAV"), "{msg}"),
        other => panic!(
            "expected clean render error, got {:?}",
            other.map(|r| r.frames)
        ),
    }
}

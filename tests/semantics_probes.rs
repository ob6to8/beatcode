//! `goldens/semantics-probes.txt` — the edge-rule transcripts cited
//! from SPEC.md: sign of zero (E1), pitch serialization on any voice
//! (E2), formatter boundary (E3), CRLF (E4), suffix trimming (E5),
//! error shapes (E6), rounding-domain edge (E8).

use bc::decfmt::{format_dec, round_dec};
use bc::error::Error;
use bc::{events, jsonl, score};

fn probes_file() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/goldens/semantics-probes.txt"
    ))
    .expect("read semantics-probes.txt")
}

fn compile_jsonl(src: &str) -> String {
    let sc = score::parse(src).expect("parse");
    let evs = events::compile(&sc).expect("compile");
    jsonl::events_jsonl(&evs)
}

fn compile_err(src: &str) -> Error {
    score::parse(src)
        .and_then(|sc| events::compile(&sc))
        .expect_err("expected an error")
}

/// Extract the indented JSONL block following the given header line.
fn block_after(file: &str, header: &str) -> Vec<String> {
    let mut lines = file.lines();
    for l in lines.by_ref() {
        if l == header {
            break;
        }
    }
    let mut out = Vec::new();
    for l in lines {
        if let Some(json) = l.strip_prefix("  {") {
            out.push(format!("{{{json}"));
        } else if !l.starts_with('#') {
            break;
        }
    }
    assert!(!out.is_empty(), "block not found after {header:?}");
    out
}

/// E1 — sign of zero through value-rounding (SPEC §6.10/§12.5):
/// negative non-zero inputs that round to zero yield +0.0; only an
/// exactly −0.0 input keeps its sign.
#[test]
fn e1_sign_of_zero_value_rounding() {
    assert_eq!(round_dec(-1.0e-4, 3).to_bits(), 0.0f64.to_bits());
    assert_eq!(round_dec(-1.0e-7, 6).to_bits(), 0.0f64.to_bits());
    assert_eq!(round_dec(-0.0, 3).to_bits(), (-0.0f64).to_bits());
    assert_eq!(round_dec(-0.125, 3).to_bits(), (-0.125f64).to_bits());
}

/// E1 end-to-end — `time [-0.0001ms]` rounds to +0.0 ("0.0"), while
/// `time [-0ms]` parses to −0.0 and stays "-0.0" in the JSONL.
#[test]
fn e1_end_to_end_lane_ms_sign() {
    let file = probes_file();
    for (entry, header) in [
        (
            "-0.0001ms",
            "tempo 120 / voice k / gate x / time [-0.0001ms]:",
        ),
        ("-0ms", "tempo 120 / voice k / gate x / time [-0ms]:"),
    ] {
        let want = block_after(&file, header);
        assert_eq!(want.len(), 16, "{header}: probe block size");
        let src = format!("tempo 120\nvoice k sample=kick\ngate x\ntime [{entry}]\n");
        let got = compile_jsonl(&src);
        let got: Vec<&str> = got.lines().collect();
        assert_eq!(got.len(), 16, "{header}: event count");
        for (g, w) in got.iter().zip(&want) {
            assert_eq!(*g, w, "{header}");
        }
    }
}

/// E2 — the pitch field serializes for ANY voice with a pitch lane
/// (SPEC §2.3): a sample voice carrying a pitch lane emits `pitch`.
#[test]
fn e2_pitch_serializes_on_sample_voice() {
    let file = probes_file();
    let want = block_after(&file, "voice k sample=kick + pitch [a1 c2] div=1/4:");
    assert_eq!(want.len(), 8, "E2 probe block size");
    let got = compile_jsonl("voice k sample=kick\ngate x.\npitch [a1 c2] div=1/4\n");
    let got: Vec<&str> = got.lines().collect();
    assert_eq!(got.len(), 8);
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(*g, w, "E2");
    }
}

/// E3 — the formatter boundary: value-rounding IS the exact rule, and
/// this implementation uses the exact rule for formatting too, so both
/// oracle `float_to_binary` lines diverge here by design (SPEC §6.10).
#[test]
fn e3_formatter_boundary() {
    assert_eq!(round_dec(5.0e-7, 6).to_bits(), 0.0f64.to_bits());
    assert_eq!(format_dec(5.0e-7, 6), "0.0"); // oracle printed 0.000001
}

/// E4 — CRLF scores parse identically to LF (SPEC §5.1, §5.10 #15).
#[test]
fn e4_crlf_equals_lf() {
    let lf = "tempo 120\nseed 1\nvoice k sample=kick\n  gate x.\n";
    let crlf = lf.replace('\n', "\r\n");
    let a = compile_jsonl(lf);
    let b = compile_jsonl(&crlf);
    assert_eq!(a.lines().count(), 8, "8 events");
    assert_eq!(a, b, "CRLF events == LF events");
}

/// E5 — "ms"/"%" trimming strips whole-suffix repetitions, not a char
/// set (SPEC §5.8, §5.10 #16).
#[test]
fn e5_suffix_trimming() {
    let ok = compile_jsonl("voice k sample=kick\n  gate x\n  hum 4.5msms\n");
    assert_eq!(ok.lines().count(), 16, "hum 4.5msms: OK, 16 events");

    for bad in ["4.5mss", "4.5sm"] {
        let e = compile_err(&format!("voice k sample=kick\n  gate x\n  hum {bad}\n"));
        assert_eq!(
            e,
            Error::Score {
                line: 3,
                msg: format!("bad number '{bad}'"),
            },
            "hum {bad}"
        );
    }

    let ok = compile_jsonl("voice k sample=kick\n  gate x\n  swing 58%%\n");
    assert_eq!(ok.lines().count(), 16, "swing 58%% (voice): OK, 16 events");
}

/// E6 — a bare `swing` raises a clean line-cited error here (the
/// reference crashed with a FunctionClauseError; decided posture,
/// SPEC §5.9).
#[test]
fn e6_bare_swing_clean_errors() {
    let e = compile_err("swing\nvoice k sample=kick\n  gate x\n");
    assert!(
        matches!(e, Error::Score { line: 1, .. }),
        "file-level bare swing: {e}"
    );
    let e = compile_err("voice k sample=kick\n  gate x\n  swing\n");
    assert!(
        matches!(e, Error::Score { line: 3, .. }),
        "voice-level bare swing: {e}"
    );
}

/// E8 — outside the §12.5 exactness domain (q′ > 2^53) the value is
/// kept, matching the oracle (see SPEC-GAPS.md).
#[test]
fn e8_rounding_domain_edge() {
    let x = 2851889632028496.5;
    assert_eq!(round_dec(x, 6).to_bits(), x.to_bits());
}

//! SPEC §11.3 item 4: parser conformance against
//! `goldens/parser-behaviors.txt` — accept/reject + cited line numbers
//! match on 40/42 cases, with exactly the two §11.2
//! expected-to-diverge cases (`bars 0`, negative `vel`) asserted as
//! *rejections*. Error texts are informative only; event summaries
//! (voice, step, grid, vel, performed_s) are checked on OK cases.

use bc::error::Error;
use bc::{events, score};

struct Case {
    header: String,
    score: String,
    expect: Expect,
}

enum Expect {
    Ok { summaries: Vec<String> },
    Raises { msg: String },
}

fn parse_transcript(src: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    let mut header = String::new();
    let mut score_lines: Vec<String> = Vec::new();
    for line in src.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            header = h.to_string();
            score_lines.clear();
        } else if let Some(rest) = line
            .strip_prefix("  | ")
            .or_else(|| (line == "  |").then_some(""))
        {
            score_lines.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("  => ") {
            let expect = if let Some(msg) = rest.strip_prefix("RAISES: ") {
                Expect::Raises {
                    msg: msg.to_string(),
                }
            } else {
                assert!(rest.starts_with("OK,"), "unexpected outcome line: {rest}");
                Expect::Ok {
                    summaries: Vec::new(),
                }
            };
            cases.push(Case {
                header: header.clone(),
                score: score_lines.join("\n"),
                expect,
            });
        } else if let Some(summary) = line.strip_prefix("    ")
            && let Some(Case {
                expect: Expect::Ok { summaries },
                ..
            }) = cases.last_mut()
        {
            summaries.push(summary.to_string());
        }
    }
    cases
}

fn run_case(case: &Case) -> Result<Vec<events::Event>, Error> {
    events::compile(&score::parse(&case.score)?)
}

/// Check one transcript summary line (`<voice> step=N grid=G vel=V
/// performed_s=P`) against an event. Fields compare textually except
/// performed_s, which parses to f64 and compares by bit pattern
/// (goldens/README.md: floats are shortest-round-trip; parse and
/// compare bit patterns).
fn check_summary(want: &str, e: &events::Event, ctx: &str) {
    let toks: Vec<&str> = want.split_whitespace().collect();
    assert_eq!(toks.len(), 5, "{ctx}: summary shape: {want}");
    assert_eq!(toks[0], e.voice, "{ctx}: voice");
    assert_eq!(toks[1], format!("step={}", e.step), "{ctx}: step");
    assert_eq!(toks[2], format!("grid={}", e.grid.to_s()), "{ctx}: grid");
    assert_eq!(toks[3], format!("vel={}", e.vel), "{ctx}: vel");
    let p: f64 = toks[4]
        .strip_prefix("performed_s=")
        .expect("performed_s=")
        .parse()
        .expect("f64");
    assert_eq!(
        e.performed_s.to_bits(),
        p.to_bits(),
        "{ctx}: performed_s got {} want {p}",
        e.performed_s
    );
}

#[test]
fn parser_behaviors_42_cases() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/goldens/parser-behaviors.txt"
    ))
    .expect("read parser-behaviors.txt");
    let cases = parse_transcript(&src);
    assert_eq!(cases.len(), 42, "expected exactly 42 cases");

    let mut diverged: Vec<&str> = Vec::new();
    for case in &cases {
        let expected_diverge =
            case.header.starts_with("bars 0:") || case.header.starts_with("negative vel parses");
        let got = run_case(case);
        if expected_diverge {
            // Decided postures (SPEC §5.10 #9/#13): both transcript-OK
            // cases must now be *rejections*.
            assert!(
                got.is_err(),
                "case {:?}: expected-to-diverge case must reject",
                case.header
            );
            diverged.push(&case.header);
            continue;
        }
        match (&case.expect, got) {
            (Expect::Ok { summaries }, Ok(evs)) => {
                assert_eq!(
                    evs.len(),
                    summaries.len(),
                    "case {:?}: event count",
                    case.header
                );
                for (i, (want, ev)) in summaries.iter().zip(&evs).enumerate() {
                    check_summary(
                        want,
                        ev,
                        &format!("case {:?}, summary line {}", case.header, i + 1),
                    );
                }
            }
            (Expect::Ok { .. }, Err(e)) => {
                panic!("case {:?}: expected OK, got error: {e}", case.header)
            }
            (Expect::Raises { msg }, Err(e)) => {
                // The cited line number is normative when the transcript
                // message carries one; texts are informative.
                if let Some(rest) = msg.strip_prefix("score error, line ") {
                    let want_line: usize = rest
                        .split(':')
                        .next()
                        .expect("line no")
                        .parse()
                        .expect("line no");
                    match e {
                        Error::Score { line, .. } => assert_eq!(
                            line,
                            want_line,
                            "case {:?}: cited line (our error: {})",
                            case.header,
                            Error::Score {
                                line,
                                msg: String::new()
                            }
                        ),
                        Error::Compile(m) => panic!(
                            "case {:?}: transcript cites line {want_line}, we raised a \
                             compile error: {m}",
                            case.header
                        ),
                    }
                }
            }
            (Expect::Raises { msg }, Ok(_)) => panic!(
                "case {:?}: expected rejection ({msg}), we accepted",
                case.header
            ),
        }
    }
    assert_eq!(
        diverged.len(),
        2,
        "exactly the two §11.2 expected-to-diverge cases: {diverged:?}"
    );
}

//! PLAN.md Phase 1: the `goldens/float-semantics.txt` probes minus its
//! two expected-to-diverge lines (the `ftb(5.0e-7)` formatter boundary,
//! SPEC §6.10, and the final `pow` probe — transcendental,
//! platform-scoped; SPEC §11.2 note 1).
//!
//! Every probe line in the file must be consumed by a handler below —
//! an unrecognized line panics, so golden drift or a lost assertion is
//! loud. Floats are compared by bit pattern.

use bc::decfmt::{format_dec, round_dec};
use bc::events::clamp0;
use bc::prng::mask_seed;
use bc::score::{int_token, num_token};

fn f(s: &str) -> f64 {
    s.trim().parse().expect("f64 literal")
}

fn bits_eq(got: f64, want: f64, ctx: &str) {
    assert_eq!(
        got.to_bits(),
        want.to_bits(),
        "{ctx}: got {got} want {want}"
    );
}

/// Evaluate a probe argument that is either a literal or `A * B`.
fn eval_product(s: &str) -> f64 {
    match s.split_once('*') {
        Some((a, b)) => f(a) * f(b),
        None => f(s),
    }
}

#[test]
fn float_semantics_probes() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/goldens/float-semantics.txt"
    ))
    .expect("read float-semantics.txt");

    let mut handled = 0;
    let mut diverged: Vec<&str> = Vec::new();

    for line in src.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        handled += 1;

        if let Some(rest) = line.strip_prefix("Float.round(") {
            let (args, want) = rest.split_once(") = ").expect("Float.round line");
            let (x, n) = args.split_once(',').expect("two args");
            let (x, n): (f64, u32) = (f(x), n.trim().parse().expect("n"));
            bits_eq(round_dec(x, n), f(want), line);
        } else if let Some(rest) = line.strip_prefix("round(") {
            // Kernel.round/1: ties away from zero — both our decimal
            // rule at n=0 and f64::round (used by accent/§9.6) obey it.
            let (arg, want) = rest.split_once(") = ").expect("round line");
            let (x, want) = (f(arg), f(want));
            bits_eq(round_dec(x, 0), want, line);
            bits_eq(x.round(), want, line);
        } else if let Some(rest) = line.strip_prefix("ftb2(") {
            let (arg, want) = rest.split_once(") = ").expect("ftb2 line");
            assert_eq!(format_dec(f(arg), 2), want, "{line}");
        } else if let Some(rest) = line.strip_prefix("ftb(") {
            let (arg, want) = rest.split_once(") = ").expect("ftb line");
            if arg == "5.0e-7" {
                // Expected-to-diverge (SPEC §6.10 formatter boundary):
                // the oracle double-rounds to "0.000001"; the exact
                // rule this implementation uses everywhere gives "0.0".
                assert_eq!(want, "0.000001");
                assert_eq!(format_dec(f(arg), 6), "0.0", "{line} (divergence pinned)");
                diverged.push(line);
            } else {
                assert_eq!(format_dec(f(arg), 6), want, "{line}");
            }
        } else if line.starts_with("Float.parse(") {
            // num! behavior on the token (SPEC §5.8): the transcript
            // documents the raw primitive; num! strips all leading '+'
            // first, so "++5" is the one accept-side difference — and
            // partial parses ({v, rest ≠ ""}) are rejections for num!'s
            // full-consumption rule.
            let expect: &[(&str, Option<f64>)] = &[
                ("\"58\"", Some(58.0)),
                ("\"58.\"", None),
                ("\".5\"", None),
                ("\"1e3\"", Some(1000.0)),
                ("\"1.0e3\"", Some(1000.0)),
                ("\"-0.02\"", Some(-0.02)),
                ("\"+5\"", Some(5.0)),
                ("\"0\"", Some(0.0)),
                ("\"1_000\"", None),
                ("\"0x10\"", None),
                ("\"++5\"", Some(5.0)), // via the leading-'+' strip
            ];
            let arg = line
                .strip_prefix("Float.parse(")
                .expect("prefix")
                .split(')')
                .next()
                .expect("arg");
            let (_, want) = expect
                .iter()
                .find(|(a, _)| *a == arg)
                .unwrap_or_else(|| panic!("unhandled Float.parse probe: {line}"));
            let tok = arg.trim_matches('"');
            match (num_token(tok), want) {
                (Some(g), Some(w)) => bits_eq(g, *w, line),
                (None, None) => {}
                (g, w) => panic!("{line}: num_token gave {g:?}, want {w:?}"),
            }
        } else if line.starts_with("Integer.parse(") {
            let expect: &[(&str, Option<i64>)] = &[
                ("\"42\"", Some(42)),
                ("\"-0\"", Some(0)),
                ("\"+2\"", Some(2)),
                ("\"3.5\"", None),
                ("\"1_000\"", None),
                ("\"16\"", Some(16)),
                ("\"++2\"", None), // int! does no extra '+' stripping
            ];
            let arg = line
                .strip_prefix("Integer.parse(")
                .expect("prefix")
                .split(')')
                .next()
                .expect("arg");
            let (_, want) = expect
                .iter()
                .find(|(a, _)| *a == arg)
                .unwrap_or_else(|| panic!("unhandled Integer.parse probe: {line}"));
            assert_eq!(int_token(arg.trim_matches('"')), *want, "{line}");
        } else if let Some(rest) = line.strip_prefix("band(") {
            let (args, want) = rest.split_once(") = ").expect("band line");
            let seed: i128 = args
                .split(',')
                .next()
                .expect("seed")
                .trim()
                .parse()
                .expect("i128");
            let want: u64 = want.parse().expect("u64");
            assert_eq!(mask_seed(seed), want, "{line}");
        } else if let Some(rest) = line.strip_prefix("trunc(") {
            let (arg, want) = rest.split_once(") = ").expect("trunc line");
            let want: i64 = want.parse().expect("i64");
            assert_eq!(eval_product(arg).trunc() as i64, want, "{line}");
        } else if let Some(rest) = line.strip_prefix("Integer.floor_div(") {
            let (args, want) = rest.split_once(") = ").expect("floor_div line");
            let mut it = args
                .split(',')
                .map(|s| s.trim().parse::<i64>().expect("i64"));
            let (a, b) = (it.next().expect("a"), it.next().expect("b"));
            assert_eq!(a.div_euclid(b), want.parse::<i64>().expect("i64"), "{line}");
        } else if let Some(rest) = line.strip_prefix("rem(") {
            let (args, want) = rest.split_once(") = ").expect("rem line");
            let mut it = args
                .split(',')
                .map(|s| s.trim().parse::<i64>().expect("i64"));
            let (a, b) = (it.next().expect("a"), it.next().expect("b"));
            assert_eq!(a % b, want.parse::<i64>().expect("i64"), "{line}");
        } else if line.starts_with("float_to_binary(max(0.0, -0.0)") {
            // The performed_s clamp: +0.0 out for −0.0 in (SPEC §6.9).
            bits_eq(clamp0(-0.0), 0.0, line);
        } else if line.starts_with("Enum.to_list(0..-1)") || line.starts_with("Enum.at(") {
            // Oracle-internal facts behind the bars=0 pathology; this
            // implementation rejects bars < 1 (SPEC §5.10 #13), so the
            // 0..-1 range and negative indexing have no analogue here.
        } else if line.contains("/ 2^64 = ") {
            let (arg, want) = line.split_once(" / 2^64 = ").expect("2^64 line");
            let u: u64 = arg.parse().expect("u64");
            bits_eq(u as f64 / 18446744073709551616.0, f(want), line);
        } else if line.starts_with("pow(-0.39, 1.5)") {
            // Expected-to-diverge (SPEC §11.2 note 1): transcendental,
            // platform-scoped — and unreachable here anyway, since
            // negative vel is rejected at parse (SPEC §5.10 #9).
            diverged.push(line);
        } else {
            panic!("unhandled probe line: {line}");
        }
    }

    assert_eq!(handled, 99, "expected 99 probe lines");
    assert_eq!(
        diverged.len(),
        2,
        "exactly the two expected-to-diverge lines: {diverged:?}"
    );
}

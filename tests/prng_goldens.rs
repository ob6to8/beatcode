//! SPEC §11.3 item 2: PRNG byte-exact against all 87
//! `goldens/prng-vectors.jsonl` entries — fnv strings, key chains
//! (compared as u64), flt outputs and noise streams (floats compared
//! by bit pattern).

mod common;

use bc::prng::{Part, flt, flt_key, fnv, mask_seed, noise};

/// Numeric text whether the golden wrote it quoted (u64 keys, floats)
/// or bare (seeds).
fn txt(v: &common::Json) -> &str {
    match v {
        common::Json::Str(s) => s,
        common::Json::Num(s) => s,
        other => panic!("expected number text, got {other:?}"),
    }
}

fn parts_of(v: &common::Json) -> Vec<OwnedPart> {
    v.arr()
        .iter()
        .map(|p| match p {
            common::Json::Str(s) => OwnedPart::Str(s.clone()),
            common::Json::Num(n) => OwnedPart::Int(n.parse().expect("int part")),
            other => panic!("unexpected part {other:?}"),
        })
        .collect()
}

enum OwnedPart {
    Str(String),
    Int(i64),
}

fn borrow(parts: &[OwnedPart]) -> Vec<Part<'_>> {
    parts
        .iter()
        .map(|p| match p {
            OwnedPart::Str(s) => Part::Str(s),
            OwnedPart::Int(i) => Part::Int(*i),
        })
        .collect()
}

#[test]
fn prng_vectors_87_byte_exact() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/goldens/prng-vectors.jsonl"
    ))
    .expect("read prng-vectors.jsonl");
    let mut count = 0;
    for line in src.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v = common::parse(line);
        count += 1;
        match v.get("fn").expect("fn").s() {
            "fnv" => {
                let input = v.get("in").expect("in").s();
                let want: u64 = txt(v.get("out").expect("out")).parse().expect("u64");
                assert_eq!(fnv(input), want, "fnv({input:?})");
            }
            "flt" => {
                let seed_raw = txt(v.get("seed").expect("seed"));
                let seed = mask_seed(seed_raw.parse::<i128>().expect("seed i128"));
                let parts = parts_of(v.get("parts").expect("parts"));
                let parts = borrow(&parts);
                let want_key: u64 = txt(v.get("key").expect("key")).parse().expect("u64 key");
                let want_out: f64 = txt(v.get("out").expect("out")).parse().expect("f64");
                assert_eq!(
                    flt_key(seed, &parts),
                    want_key,
                    "key(seed={seed_raw}, parts=…)"
                );
                let got = flt(seed, &parts);
                assert_eq!(
                    got.to_bits(),
                    want_out.to_bits(),
                    "flt(seed={seed_raw}) got {got} want {want_out}"
                );
            }
            "noise" => {
                let tag = v.get("tag").expect("tag").s();
                let first8 = v.get("first8").expect("first8").arr();
                let got = noise(tag, first8.len());
                for (i, w) in first8.iter().enumerate() {
                    let want: f64 = txt(w).parse().expect("f64");
                    assert_eq!(
                        got[i].to_bits(),
                        want.to_bits(),
                        "noise({tag:?})[{i}] got {} want {want}",
                        got[i]
                    );
                }
            }
            other => panic!("unknown vector fn {other:?}"),
        }
    }
    assert_eq!(count, 87, "expected exactly 87 vectors");
}

/// Keyed property (SPEC §4.6): a prefix of a longer stream equals a
/// shorter stream.
#[test]
fn noise_prefix_property() {
    let long = noise("snare", 64);
    let short = noise("snare", 8);
    assert_eq!(&long[..8], &short[..]);
}

/// SPEC §4.4: the range includes 1.0 — 2^64−1 maps to exactly 1.0
/// after round-to-nearest u64→f64 conversion (also probed in
/// float-semantics.txt).
#[test]
fn u64_to_f64_conversion_probes() {
    assert_eq!(
        (18446744073709551615_u64 as f64 / 18446744073709551616.0),
        1.0
    );
    assert_eq!(
        (9007199254740993_u64 as f64 / 18446744073709551616.0).to_bits(),
        4.8828125e-4_f64.to_bits()
    );
    assert_eq!(
        (6148914691236517205_u64 as f64 / 18446744073709551616.0).to_bits(),
        0.3333333333333333_f64.to_bits()
    );
}

/// SPEC §2.1/§12.6: seed masking is two's-complement to u64.
#[test]
fn seed_masking_probes() {
    assert_eq!(mask_seed(-1), 18446744073709551615);
    assert_eq!(mask_seed(-2), 18446744073709551614);
    assert_eq!(mask_seed(18446744073709551616), 0);
}

//! The keyed PRNG (SPEC §4) — byte-exact required.
//!
//! Every value is derived from *what it is for* (fnv-1a key chain via
//! decimal string formatting, splitmix64 finalizer), so edits elsewhere
//! in a score never reshuffle unrelated jitter.

use std::fmt;

/// fnv-1a (64-bit) over the UTF-8 bytes of the input (SPEC §4.1).
pub fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xCBF29CE484222325; // offset basis = fnv("")
    for b in s.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001B3);
    }
    h
}

/// A key-chain part (SPEC §4.2): strings render verbatim (no quotes),
/// integers in decimal (with `-` if negative).
#[derive(Clone, Copy, Debug)]
pub enum Part<'a> {
    Str(&'a str),
    Int(i64),
}

impl fmt::Display for Part<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Part::Str(s) => f.write_str(s),
            Part::Int(i) => write!(f, "{i}"),
        }
    }
}

/// The string-mixed key chain (SPEC §4.2). `acc` always renders in
/// unsigned decimal (SPEC §12.4 trap 7).
pub fn flt_key(seed: u64, parts: &[Part]) -> u64 {
    let mut acc = seed;
    for p in parts {
        acc = fnv(&format!("{p}|{acc}"));
    }
    acc
}

/// splitmix64 finalizer, applied to the key once (SPEC §4.3).
pub fn splitmix64(key: u64) -> u64 {
    let s = key.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = (s ^ (s >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// u64 → f64 in `[0.0, 1.0]` **inclusive** (SPEC §4.4): `as f64`
/// rounds to nearest (ties even), so outputs ≥ 2^64 − 2^10 round up to
/// 2^64 and the quotient reaches exactly 1.0. Do not "fix" this.
pub fn flt(seed: u64, parts: &[Part]) -> f64 {
    splitmix64(flt_key(seed, parts)) as f64 / 18446744073709551616.0
}

/// Noise streams (SPEC §4.6): values in `[−1.0, 1.0]`, keyed on
/// `(tag, i)` only — independent of the score seed.
pub fn noise(tag: &str, n: usize) -> Vec<f64> {
    let base = fnv(&format!("sample|{tag}"));
    (0..n)
        .map(|i| flt(base, &[Part::Int(i as i64)]) * 2.0 - 1.0)
        .collect()
}

/// Mask a score seed to u64 two's-complement (SPEC §2.1, §12.6):
/// −1 → 18446744073709551615; 2^64 → 0.
pub fn mask_seed(seed: i128) -> u64 {
    seed.rem_euclid(1_i128 << 64) as u64
}

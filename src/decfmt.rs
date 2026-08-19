//! Exact decimal rounding and formatting (SPEC §6.10, §7, §12.5).
//!
//! One integer routine serves both value-rounding (n = 3, 6, 2, and 0
//! for the ties-away probes) and JSONL formatting (n = 6 + compact
//! trim). It rounds the **exact binary value** of the f64 to n decimal
//! digits, half away from zero on exact ties (reachable only at dyadic
//! points) — never the "school" answer off a decimal literal, and never
//! Rust's `{:.N}` (which ties to even; SPEC §12.4 trap 2).
//!
//! Domain: |x| ≤ 2^53 / 10^n. Outside it the final conversion would no
//! longer be exact; we return/format the value unchanged there, which
//! matches the reference (`goldens/semantics-probes.txt` §E8 — the
//! oracle keeps the value) and keeps §5.9's clean-error rule (see
//! SPEC-GAPS.md).

/// (negative, mantissa m, exponent e) with x = ±m·2^e.
fn decompose(x: f64) -> (bool, u64, i64) {
    let bits = x.to_bits();
    let neg = bits >> 63 == 1;
    let exp_field = ((bits >> 52) & 0x7FF) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    if exp_field == 0 {
        (neg, frac, -1074) // subnormal (or zero)
    } else {
        (neg, frac | (1u64 << 52), exp_field - 1075)
    }
}

fn pow10(n: u32) -> u128 {
    10u128.pow(n)
}

/// Integer |x|·10^n rounded half away from zero, or None outside the
/// exactness domain |x| ≤ 2^53/10^n.
fn qprime(x: f64, n: u32) -> Option<u128> {
    if x.abs() > 9007199254740992.0 / pow10(n) as f64 {
        return None;
    }
    let (_, m, e) = decompose(x);
    // N = m·10^n fits u128: 2^53·10^6 < 2^73.
    let mut big_n = u128::from(m) * pow10(n);
    let k = if e > 0 {
        // Only reachable at |x| = 2^53 exactly (n = 0): fold the shift in.
        big_n <<= e as u32;
        0
    } else {
        (-e) as u32
    };
    let q = if k < 128 { big_n >> k } else { 0 };
    // Top-dropped-bit test: r = ½ has the top bit set (round up — away),
    // r > ½ likewise, r < ½ clear — round-half-away exactly (SPEC §12.5).
    let round_up = match k {
        0 => false,
        1..=127 => (big_n >> (k - 1)) & 1 == 1,
        128 => big_n >= 1u128 << 127,
        _ => false, // value too small to reach the half point
    };
    Some(q + u128::from(round_up))
}

/// Value-rounding (SPEC §6.10). Sign of zero: a negative non-zero value
/// that rounds to zero yields +0.0; −0.0 results only from an exactly
/// −0.0 input (probed: semantics-probes §E1).
pub fn round_dec(x: f64, n: u32) -> f64 {
    let Some(q1) = qprime(x, n) else {
        return x; // out of domain: keep the value (§E8)
    };
    let (neg, ..) = decompose(x);
    if q1 == 0 {
        return if x == 0.0 && neg { -0.0 } else { 0.0 };
    }
    // Both operands exact, one correctly-rounded division.
    let val = q1 as f64 / pow10(n) as f64;
    if neg { -val } else { val }
}

/// Formatting (SPEC §7, §12.5): digits of q′ zero-padded to n+1, split
/// n from the right, '.' inserted, trailing zeros trimmed keeping ≥ 1
/// fractional digit. '-' is prepended iff the f64 being formatted is
/// negative — a stored −0.0 formats as "-0.0", and a verbatim negative
/// tiny value (e.g. `pan=-1.0e-7`) formats as "-0.0" too (golden:
/// `ftb(-1.0e-7) = -0.0`), unlike value-rounding.
pub fn format_dec(x: f64, n: u32) -> String {
    let Some(q1) = qprime(x, n) else {
        return format!("{x}"); // out of domain: shortest round-trip (SPEC-GAPS)
    };
    let (neg, ..) = decompose(x);
    let digits = format!("{:0>width$}", q1, width = n as usize + 1);
    let split = digits.len() - n as usize;
    let (int_part, frac_part) = digits.split_at(split);
    let frac_trimmed = frac_part.trim_end_matches('0');
    let frac = if frac_trimmed.is_empty() {
        "0"
    } else {
        frac_trimmed
    };
    let sign = if neg { "-" } else { "" };
    format!("{sign}{int_part}.{frac}")
}

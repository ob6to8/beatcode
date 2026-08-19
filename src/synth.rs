//! Synthesis — the kit (SPEC §8), realized transcendental-free
//! (SPEC §8.4) so the render path is bit-exact across machines
//! (Class C). The recipes follow the reference characterization's
//! shapes; matching the reference's audio byte-for-byte is a non-goal
//! (Class B).
//!
//! Every constant that the reference computed with `exp`/`pow` is
//! precomputed here as an f64 literal with its derivation, evaluated
//! at 60-digit precision and rounded to nearest f64. Envelopes decay
//! multiplicatively: `env *= K` per sample with `K = e^(−1/(sr·τ))`,
//! so `env` at sample i equals `e^(−i/(sr·τ))` up to accumulated
//! (deterministic) rounding; `env` is used first, then decayed, so
//! sample 0 sees exactly 1.0.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::events::Kind;
use crate::prng::noise;
use crate::score::{SampleKind, parse_note};

const SR: f64 = 44100.0;
// 2π as a constant: TAU is exactly 2 × PI in f64 (an exponent shift),
// both std literals — no libm call (SPEC §8.4).
const TAU: f64 = std::f64::consts::TAU;

// Envelope decay factors, K = e^(−1/(44100·τ)) for the recipe's τ set:
const K_KICK_AMP: f64 = 0.999802839117358; // τ = 0.115 (kick amplitude)
const K_KICK_SWEEP: f64 = 0.9994332672296815; // τ = 0.040 (kick pitch sweep)
const K_SNARE_TONE: f64 = 0.9995877988516744; // τ = 0.055 (snare tone)
const K_SNARE_NOISE: f64 = 0.9997165934552059; // τ = 0.080 (snare noise)
const K_HAT: f64 = 0.9989207857728373; // τ = 0.021 (hat)
const K_CLAP_TAIL: f64 = 0.9996512033519847; // τ = 0.065 (clap tail)
const K_PLUCK: f64 = 0.9998488398461045; // τ = 0.150 (pluck)

// Clap tail start: the first sample with t > 0.028 is i = 1235
// (1234/44100 = 0.02798…, 1235/44100 = 0.02800453…), so the tail
// envelope starts at e^(−(1235/44100 − 0.028)/0.065):
const CLAP_TAIL0: f64 = 0.9999302309356315;

// Odd-polynomial sine coefficients: the Taylor series of sin(z) about
// 0, exact-rational coefficients (−1)^k/(2k+1)! as const-evaluated
// divisions of f64 literals. On the reduced range |z| ≤ π/2 the first
// omitted term bounds the error: (π/2)^15/15! ≈ 6.6e−10 — within the
// "accuracy ≈ 1e-9 is ample" budget of SPEC §8.4.
const C3: f64 = -1.0 / 6.0; // 1/3!
const C5: f64 = 1.0 / 120.0; // 1/5!
const C7: f64 = -1.0 / 5040.0; // 1/7!
const C9: f64 = 1.0 / 362880.0; // 1/9!
const C11: f64 = -1.0 / 39916800.0; // 1/11!
const C13: f64 = 1.0 / 6227020800.0; // 1/13!

/// Pinned sine: reduce to cycles, wrap to [−0.5, 0.5), fold the outer
/// quarters onto the inner half-wave (sin(2πu) = sin(2π(±0.5 − u))),
/// then evaluate the odd polynomial on z = 2πu ∈ [−π/2, π/2]. Only
/// IEEE basic ops and floor — deterministic everywhere.
pub fn sin_p(x: f64) -> f64 {
    let u = x * (1.0 / TAU);
    let u = u - (u + 0.5).floor();
    let u = if u > 0.25 {
        0.5 - u
    } else if u < -0.25 {
        -0.5 - u
    } else {
        u
    };
    let z = u * TAU;
    let z2 = z * z;
    z * (1.0 + z2 * (C3 + z2 * (C5 + z2 * (C7 + z2 * (C9 + z2 * (C11 + z2 * C13))))))
}

/// cos(x) = sin(x + π/2) with π/2 as the f64 literal (SPEC §8.4).
pub fn cos_p(x: f64) -> f64 {
    sin_p(x + std::f64::consts::FRAC_PI_2)
}

/// Pinned 2^(1/12) (SPEC §5.6).
pub const SEMITONE: f64 = 1.0594630943592953;

/// `freq = 440 · r^(midi−69)` via an integer-exponent loop on the
/// pinned literal — repeated multiply up, repeated divide down; every
/// step a correctly-rounded basic op (SPEC §5.6, §8.4).
pub fn note_freq(midi: i64) -> f64 {
    let mut f = 440.0_f64;
    let k = midi - 69;
    if k >= 0 {
        for _ in 0..k {
            f *= SEMITONE;
        }
    } else {
        for _ in 0..-k {
            f /= SEMITONE;
        }
    }
    f
}

fn kick() -> Vec<f64> {
    let len = (0.30 * SR) as usize; // trunc: 13230
    let click_noise = noise("kick-click", 40);
    let mut out = Vec::with_capacity(len);
    let mut ph = 0.0_f64;
    let mut sweep = 76.0_f64; // f(t) = 44 + 76·e^(−t/0.040): 120→44 Hz
    let mut env = 1.0_f64; // e^(−t/0.115)
    for i in 0..len {
        let f = 44.0 + sweep;
        ph += TAU * f / SR; // phase pre-increments (SPEC §8)
        // click for i < 40 only (the noise stream is 40 samples long)
        let click = click_noise
            .get(i)
            .map_or(0.0, |&n| n * 0.35 * (1.0 - i as f64 / 40.0));
        out.push(sin_p(ph) * env * 0.95 + click);
        sweep *= K_KICK_SWEEP;
        env *= K_KICK_AMP;
    }
    out
}

fn snare() -> Vec<f64> {
    let len = (0.22 * SR) as usize; // trunc: 9702
    let n = noise("snare", len);
    let mut out = Vec::with_capacity(len);
    let mut ph = 0.0_f64;
    let mut env_tone = 1.0_f64; // e^(−t/0.055)
    let mut env_noise = 1.0_f64; // e^(−t/0.080)
    for x in n {
        ph += TAU * 186.0 / SR;
        out.push(sin_p(ph) * 0.45 * env_tone + x * 0.80 * env_noise);
        env_tone *= K_SNARE_TONE;
        env_noise *= K_SNARE_NOISE;
    }
    out
}

fn hat() -> Vec<f64> {
    let len = (0.075 * SR) as usize; // trunc: 3307
    let n = noise("hat", len);
    let mut out = Vec::with_capacity(len);
    let (mut y1, mut x1) = (0.0_f64, 0.0_f64);
    let mut env = 1.0_f64; // e^(−t/0.021)
    for x in n {
        // One-pole highpass, state init (0, 0).
        let y = 0.92 * (y1 + x - x1);
        x1 = x;
        y1 = y;
        out.push(y * 0.7 * env);
        env *= K_HAT;
    }
    out
}

fn clap() -> Vec<f64> {
    let len = (0.26 * SR) as usize; // trunc: 11466
    let n = noise("clap", len);
    let mut out = Vec::with_capacity(len);
    // Three bursts at trunc(0·sr), trunc(0.011·sr), trunc(0.023·sr),
    // each window [b, b+352) where 352 = trunc(0.008·sr). Windows are
    // disjoint but the *sum* form is the spec.
    const BURSTS: [usize; 3] = [0, 485, 1014];
    const BURST_LEN: usize = 352;
    let mut tail_env = CLAP_TAIL0; // value at i = 1235, first t > 0.028
    for (i, &x) in n.iter().enumerate() {
        let t = i as f64 / SR;
        let mut acc = 0.0_f64;
        for b in BURSTS {
            if i >= b && i < b + BURST_LEN {
                acc += x * 0.75;
            }
        }
        // Tail for t > 0.028 — overlaps and adds with the third burst.
        if t > 0.028 {
            acc += x * 0.55 * tail_env;
            tail_env *= K_CLAP_TAIL;
        }
        out.push(acc);
    }
    out
}

fn pluck(freq: f64) -> Vec<f64> {
    let len = (0.45 * SR) as usize; // trunc: 19845
    let mut out = Vec::with_capacity(len);
    let mut ph = 0.0_f64;
    let mut env = 1.0_f64; // e^(−t/0.150)
    let mut lp = 0.0_f64;
    for _ in 0..len {
        ph += TAU * freq / SR;
        let cyc = ph / TAU;
        let saw = 2.0 * (cyc - cyc.floor()) - 1.0;
        let raw = (saw * 0.7 + sin_p(2.0 * ph) * 0.15) * env;
        lp += 0.18 * (raw - lp); // one-pole lowpass
        out.push(lp);
        env *= K_PLUCK;
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Key {
    Sample(u8),
    Pluck(i64), // round(freq × 100), ties away — 0.01 Hz quantization
}

/// Memoized kit. The cache is lookup-only (never iterated), so any map
/// type is fine (SPEC §8); BTreeMap keeps even that surface ordered.
#[derive(Default)]
pub struct Kit {
    cache: BTreeMap<Key, Rc<Vec<f64>>>,
}

impl Kit {
    pub fn new() -> Kit {
        Kit::default()
    }

    /// The buffer for an event's kind and (for pluck) pitch name. A
    /// pluck voice with no pitch lane synthesizes at 110.0 Hz.
    pub fn buffer(&mut self, kind: Kind, pitch: Option<&str>) -> Rc<Vec<f64>> {
        let (key, freq) = match kind {
            Kind::Sample(s) => (Key::Sample(s as u8), 0.0),
            Kind::Pluck => {
                let freq = match pitch {
                    Some(name) => {
                        let midi = parse_note(name).expect("pitch validated at parse");
                        note_freq(midi)
                    }
                    None => 110.0,
                };
                (Key::Pluck((freq * 100.0).round() as i64), freq)
            }
        };
        if let Some(b) = self.cache.get(&key) {
            return Rc::clone(b);
        }
        let buf = Rc::new(match kind {
            Kind::Sample(SampleKind::Kick) => kick(),
            Kind::Sample(SampleKind::Snare) => snare(),
            Kind::Sample(SampleKind::Hat) => hat(),
            Kind::Sample(SampleKind::Clap) => clap(),
            Kind::Pluck => pluck(freq),
        });
        self.cache.insert(key, Rc::clone(&buf));
        buf
    }
}

//! Compilation to events (SPEC §6) — the Class A pipeline.
//!
//! Every f64 expression here is evaluated in strict left-to-right
//! association — no reordering, no FMA (SPEC §6.2). The rounded values
//! *are* the event: round first, then sort/place (SPEC §2.3).

use crate::decfmt::round_dec;
use crate::error::Error;
use crate::prng::{Part, flt, mask_seed};
use crate::rational::{RatError, Rational};
use crate::score::{GateChar, Lane, SampleKind, Score, Swing, TimeEntry, Voice, VoiceSwing};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Sample(SampleKind),
    Pluck,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Sample(s) => s.name(),
            Kind::Pluck => "pluck",
        }
    }
}

/// The keystone contract (SPEC §2.3). Float fields hold the **rounded**
/// values (ms at 3 decimals, performed_s at 6).
#[derive(Clone, Debug)]
pub struct Event {
    pub voice: String,
    pub kind: Kind,
    pub step: i64,
    pub grid: Rational,
    pub vel: i64,
    pub swing_ms: f64,
    pub lane_ms: f64,
    pub hum_ms: f64,
    pub performed_s: f64,
    pub pitch: Option<String>,
    pub gain: f64,
    pub pan: f64,
}

fn rat_err(e: RatError) -> Error {
    Error::Compile(e.msg().to_string())
}

/// Lane indexing — time-indexed, not event-indexed (SPEC §6.3).
fn lane_idx<T>(lane: &Lane<T>, grid: Rational) -> Result<usize, Error> {
    let q = grid.divr(lane.div).map_err(rat_err)?;
    Ok(q.floor_i().rem_euclid(lane.vals.len() as i64) as usize)
}

/// Swing offset, MPC convention (SPEC §6.5): only events landing
/// exactly on odd integer multiples of the subdivision are delayed.
fn swing_offset(eff: Option<Swing>, grid: Rational, spb: f64) -> Result<f64, Error> {
    let Some(sw) = eff else { return Ok(0.0) };
    let q = grid.divr(sw.sub).map_err(rat_err)?;
    if q.is_int() && q.floor_i() % 2 != 0 {
        let pair_s = (2.0 * sw.sub.to_f()) * spb; // left-to-right, §6.2
        Ok((sw.amount / 100.0 - 0.5) * pair_s)
    } else {
        Ok(0.0)
    }
}

/// Time-lane offset (SPEC §6.6).
fn time_offset(lane: Option<&Lane<TimeEntry>>, grid: Rational, step_s: f64) -> Result<f64, Error> {
    let Some(lane) = lane else { return Ok(0.0) };
    Ok(match lane.vals[lane_idx(lane, grid)?] {
        TimeEntry::Ms(v) => v / 1000.0,
        TimeEntry::Frac(v) => v * step_s,
    })
}

/// `max(0.0, x)` returning +0.0 for x = −0.0 (SPEC §6.9), written
/// branchily because IEEE maxNum on (±0, ∓0) is not fully specified.
pub fn clamp0(x: f64) -> f64 {
    if x > 0.0 { x } else { 0.0 }
}

pub fn compile(sc: &Score) -> Result<Vec<Event>, Error> {
    if sc.tempo == 0.0 {
        // Decided posture (SPEC §5.10 #11): clean error instead of the
        // reference's bare arithmetic crash. Negative tempo stays
        // accepted (all performed_s clamp to 0.0).
        return Err(Error::Compile("tempo must not be zero".to_string()));
    }
    if sc.bars < 1 {
        // Decided posture (SPEC §5.10 #13): reject bars < 1.
        return Err(Error::Compile("bars must be at least 1".to_string()));
    }
    let spb = 60.0 / sc.tempo;
    // A subnormal tempo overflows spb to inf; the oracle's arithmetic
    // raises there (like tempo 0), so reject cleanly (SPEC-GAPS #7).
    if !spb.is_finite() {
        return Err(Error::Compile("tempo out of range".to_string()));
    }
    let pattern = Rational::new(sc.bars, 1)
        .and_then(|b| b.mul(Rational::new(4, 1).expect("4/1")))
        .map_err(rat_err)?;
    let seed = mask_seed(sc.seed);

    let mut events = Vec::new();
    for v in &sc.voices {
        compile_voice(v, sc, pattern, spb, seed, &mut events)?;
    }

    // Ascending by (performed_s, voice bytes, step); stable, so ties
    // beyond the triple (duplicate voice names) keep score order
    // (SPEC §6.11 — this order is also the mix accumulation order).
    events.sort_by(|a, b| {
        a.performed_s
            .partial_cmp(&b.performed_s)
            .expect("performed_s is finite")
            .then_with(|| a.voice.as_bytes().cmp(b.voice.as_bytes()))
            .then_with(|| a.step.cmp(&b.step))
    });
    Ok(events)
}

fn compile_voice(
    v: &Voice,
    sc: &Score,
    pattern: Rational,
    spb: f64,
    seed: u64,
    events: &mut Vec<Event>,
) -> Result<(), Error> {
    let gate = v
        .gate
        .as_ref()
        .ok_or_else(|| Error::Compile(format!("voice {} has no gate", v.name)))?;
    let n_steps_r = pattern.divr(v.clock).map_err(rat_err)?;
    if !n_steps_r.is_int() {
        return Err(Error::Compile(format!(
            "voice {}: clock does not divide pattern",
            v.name
        )));
    }
    let n_steps = n_steps_r.floor_i();
    let step_s = v.clock.to_f() * spb;
    let eff_swing = match v.swing {
        VoiceSwing::Inherit => sc.swing,
        VoiceSwing::Off => None,
        VoiceSwing::Set(s) => Some(s),
    };
    let kind = match v.sample {
        Some(s) => Kind::Sample(s), // if both given, sample wins (§2.2)
        None => Kind::Pluck,
    };

    for i in 0..n_steps {
        // rem_euclid keeps the index target-width-independent (a bare
        // `i as usize` would truncate on 32-bit targets once i ≥ 2^32,
        // breaking Class A cross-platform byte-exactness).
        let ch = gate[i.rem_euclid(gate.len() as i64) as usize];
        if ch == GateChar::Rest {
            continue;
        }
        // Keyed PRNG makes draw order irrelevant; the short-circuit at
        // prob ≥ 1.0 only avoids the draw (SPEC §6.8).
        let keep_prob = v.prob >= 1.0
            || flt(seed, &[Part::Str(&v.name), Part::Str("prob"), Part::Int(i)]) < v.prob;
        if !keep_prob {
            continue;
        }
        let grid = v
            .clock
            .mul(Rational::new(i, 1).map_err(rat_err)?)
            .map_err(rat_err)?;
        let vel0 = match &v.vel {
            Some(lane) => lane.vals[lane_idx(lane, grid)?],
            None => 100,
        };
        // Accent: ×1.15, round half away from zero, cap 127 (SPEC §6.4).
        let vel = if ch == GateChar::Accent {
            127.min((vel0 as f64 * 1.15).round() as i64)
        } else {
            vel0
        };
        let swing_s = swing_offset(eff_swing, grid, spb)?;
        let lane_s = time_offset(v.time.as_ref(), grid, step_s)?;
        let hum_s = if v.hum_ms > 0.0 {
            let draw = flt(seed, &[Part::Str(&v.name), Part::Str("hum"), Part::Int(i)]);
            ((2.0 * draw - 1.0) * v.hum_ms) / 1000.0
        } else {
            0.0
        };
        let pitch = match &v.pitch {
            Some(lane) => Some(lane.vals[lane_idx(lane, grid)?].clone()),
            None => None,
        };
        let performed = clamp0(((grid.to_f() * spb + swing_s) + lane_s) + hum_s);
        // All literals are finite (§5.8 num!), but extreme-magnitude
        // combinations can still overflow to ±inf (and 0·inf to NaN)
        // mid-pipeline — including at the ×1000 ms scaling, where a
        // finite seconds value can overflow. The oracle raises on any
        // such operation; keep §2.3's finiteness contract with a clean
        // error by checking the STORED (rounded, ms-scale) fields —
        // round_dec passes non-finite values through untouched, so the
        // check sees exactly what JSONL would serialize (SPEC-GAPS #7).
        let swing_ms = round_dec(swing_s * 1000.0, 3);
        let lane_ms = round_dec(lane_s * 1000.0, 3);
        let hum_ms = round_dec(hum_s * 1000.0, 3);
        let performed_s = round_dec(performed, 6);
        if !(swing_ms.is_finite()
            && lane_ms.is_finite()
            && hum_ms.is_finite()
            && performed_s.is_finite())
        {
            return Err(Error::Compile(format!(
                "voice {}: non-finite event timing (arithmetic overflow)",
                v.name
            )));
        }
        events.push(Event {
            voice: v.name.clone(),
            kind,
            step: i,
            grid,
            vel,
            swing_ms,
            lane_ms,
            hum_ms,
            performed_s,
            pitch,
            gain: v.gain,
            pan: v.pan,
        });
    }
    Ok(())
}

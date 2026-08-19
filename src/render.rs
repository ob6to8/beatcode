//! Mixdown and WAV (SPEC §9): placement from the *rounded*
//! `performed_s`, event-then-frame summation order, `last + 22050`
//! tail, conditional peak normalization, symmetric ±32767 ties-away
//! s16, LE interleave. No reassociation, no SIMD, no `mul_add`.

use crate::events::Event;
use crate::synth::{Kit, cos_p, sin_p};
use crate::{sha256, wav};

pub struct Render {
    pub wav_bytes: Vec<u8>,
    pub frames: usize,
    pub events: usize,
    pub peak_normalized: bool,
    pub sha256_hex: String,
}

pub fn render(events: &[Event]) -> Render {
    let mut kit = Kit::new();
    let mut mix: Vec<(f64, f64)> = Vec::new();
    let mut last: Option<usize> = None; // highest frame index touched

    // Placement + accumulation in sorted-event order, frame order
    // within each event's buffer (SPEC §9.1/§9.3 — order is normative;
    // float addition does not commute in rounding).
    for e in events {
        let buf = kit.buffer(e.kind, e.pitch.as_deref());
        // f0 from the ROUNDED 6-decimal performed_s (SPEC §12.4 trap 4).
        let f0 = (e.performed_s * 44100.0).trunc() as usize;
        // amp = pow(vel/127, 1.5) × gain, realized as x·sqrt(x) —
        // sqrt is an exactly-specified IEEE op (SPEC §8.4, §9.2).
        let x = e.vel as f64 / 127.0;
        let amp = x * x.sqrt() * e.gain;
        // Constant-power pan: −1 → 0, 0 → π/4, +1 → π/2; values beyond
        // ±1 leave the first quadrant and phase-flip a channel.
        let ang = (e.pan + 1.0) * std::f64::consts::FRAC_PI_4;
        let a_l = amp * cos_p(ang);
        let a_r = amp * sin_p(ang);
        let end = f0 + buf.len();
        if mix.len() < end {
            mix.resize(end, (0.0, 0.0));
        }
        for (j, s) in buf.iter().enumerate() {
            mix[f0 + j].0 += s * a_l;
            mix[f0 + j].1 += s * a_r;
        }
        last = Some(last.map_or(end - 1, |l| l.max(end - 1)));
    }

    // Track length (SPEC §9.4): last + trunc(0.5·44100); an empty mix
    // is one second of silence.
    let frames = match last {
        Some(l) => l + 22050,
        None => 44100,
    };
    mix.resize(frames, (0.0, 0.0));

    // Peak scan is order-free (max); normalization is strictly > 0.98
    // targeting 0.98 (SPEC §9.5).
    let mut peak = 0.0_f64;
    for &(l, r) in &mix {
        peak = peak.max(l.abs()).max(r.abs());
    }
    let peak_normalized = peak > 0.98;
    let norm = if peak_normalized { 0.98 / peak } else { 1.0 };

    // Quantization (SPEC §9.6): symmetric ±32767, ties away from zero
    // (f64::round is ties-away and exactly specified).
    let s16 = |v: f64| ((v * norm).clamp(-1.0, 1.0) * 32767.0).round() as i16;
    let frames_s16: Vec<(i16, i16)> = mix.iter().map(|&(l, r)| (s16(l), s16(r))).collect();

    let wav_bytes = wav::file_bytes(&frames_s16);
    // The receipt: sha256 over the ENTIRE file bytes (SPEC §9.8).
    let sha256_hex = sha256::hex(&wav_bytes);
    Render {
        wav_bytes,
        frames,
        events: events.len(),
        peak_normalized,
        sha256_hex,
    }
}

//! Score format parser (SPEC §5) and the score data model (SPEC §2).

use crate::error::Error;
use crate::rational::Rational;

#[derive(Clone, Debug)]
pub struct Score {
    pub tempo: f64,
    pub bars: i64,
    pub seed: i128,
    pub swing: Option<Swing>,
    pub voices: Vec<Voice>,
}

#[derive(Clone, Copy, Debug)]
pub struct Swing {
    pub amount: f64,
    pub sub: Rational, // beats
}

/// Genuinely three-state (SPEC §2.2): a two-state Option cannot
/// represent "swing off under a global swing".
#[derive(Clone, Copy, Debug)]
pub enum VoiceSwing {
    Inherit,
    Off,
    Set(Swing),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SampleKind {
    Kick,
    Snare,
    Hat,
    Clap,
}

impl SampleKind {
    pub fn name(self) -> &'static str {
        match self {
            SampleKind::Kick => "kick",
            SampleKind::Snare => "snare",
            SampleKind::Hat => "hat",
            SampleKind::Clap => "clap",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GateChar {
    Hit,    // x
    Accent, // X
    Rest,   // .
}

#[derive(Clone, Copy, Debug)]
pub enum TimeEntry {
    Ms(f64),
    Frac(f64),
}

#[derive(Clone, Debug)]
pub struct Lane<T> {
    pub vals: Vec<T>,  // non-empty
    pub div: Rational, // beats
}

#[derive(Clone, Debug)]
pub struct Voice {
    pub name: String,
    pub sample: Option<SampleKind>,
    pub synth_pluck: bool,
    pub gain: f64,
    pub pan: f64,
    pub clock: Rational, // beats; default 1/4 beat = a sixteenth note
    pub gate: Option<Vec<GateChar>>,
    pub vel: Option<Lane<i64>>,
    pub pitch: Option<Lane<String>>, // note names as written
    pub time: Option<Lane<TimeEntry>>,
    pub hum_ms: f64,
    pub prob: f64,
    pub swing: VoiceSwing,
}

impl Voice {
    fn new(name: String) -> Voice {
        Voice {
            name,
            sample: None,
            synth_pluck: false,
            gain: 1.0,
            pan: 0.0,
            clock: Rational::new(1, 4).expect("1/4"),
            gate: None,
            vel: None,
            pitch: None,
            time: None,
            hum_ms: 0.0,
            prob: 1.0,
            swing: VoiceSwing::Inherit,
        }
    }
}

fn err(line: usize, msg: String) -> Error {
    Error::Score { line, msg }
}

/// num! (SPEC §5.8): strip **all** leading '+', then require full
/// consumption of `sign? digits ('.' digits)? ([eE] sign? digits)?`.
/// So `++5` parses (via the strip) where the raw primitive would not.
/// Overflowing literals (`1e999`) are rejected: the oracle's floats
/// have no infinities — `Float.parse` errors on overflow — and a
/// non-finite value would break §2.3's finiteness contract downstream
/// (SPEC-GAPS #7).
pub fn num_token(tok: &str) -> Option<f64> {
    let s = tok.trim_start_matches('+');
    if !num_grammar(s) {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn parse_num(tok: &str, orig: &str, line: usize) -> Result<f64, Error> {
    num_token(tok).ok_or_else(|| err(line, format!("bad number '{orig}'")))
}

fn num_grammar(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    let d0 = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == d0 {
        return false;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let d1 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == d1 {
            return false;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let d2 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == d2 {
            return false;
        }
    }
    i == b.len()
}

/// int! (SPEC §5.8): decimal integer, full consumption, one optional
/// leading sign — no extra '+' stripping.
fn int_grammar(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let d0 = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i > d0 && i == b.len()
}

/// int! (SPEC §5.8) as a fallible token parse (None = reject).
pub fn int_token(tok: &str) -> Option<i64> {
    if !int_grammar(tok) {
        return None;
    }
    tok.trim_start_matches('+').parse::<i64>().ok()
}

fn parse_int(tok: &str, line: usize) -> Result<i64, Error> {
    int_token(tok).ok_or_else(|| err(line, format!("bad integer '{tok}'")))
}

fn parse_int_i128(tok: &str, line: usize) -> Result<i128, Error> {
    if !int_grammar(tok) {
        return Err(err(line, format!("bad integer '{tok}'")));
    }
    tok.trim_start_matches('+')
        .parse::<i128>()
        .map_err(|_| err(line, format!("bad integer '{tok}'")))
}

/// ms! (SPEC §5.8): whole-suffix "ms" repetitions trimmed, then num!.
/// `4.5msms` → 4.5, but `4.5mss` and `4.5sm` are rejected (§E5).
fn parse_ms(tok: &str, line: usize) -> Result<f64, Error> {
    let mut s = tok;
    while let Some(rest) = s.strip_suffix("ms") {
        s = rest;
    }
    parse_num(s, tok, line)
}

/// Swing amount: trailing '%' characters trimmed (single-char suffix,
/// so `58%%` → 58.0), then num!.
fn parse_pct(tok: &str, line: usize) -> Result<f64, Error> {
    let s = tok.trim_end_matches('%');
    parse_num(s, tok, line)
}

/// Note-value fraction → beats (SPEC §5.7): `n/d` of a whole note,
/// `beats = 4n/d`. Zero or negative notefracs are rejected (§5.10 #12).
fn parse_notefrac(tok: &str, line: usize) -> Result<Rational, Error> {
    let bad = || err(line, format!("bad note fraction '{tok}' (want e.g. 1/16)"));
    let (n_s, d_s) = tok.split_once('/').ok_or_else(bad)?;
    if !int_grammar(n_s) || !int_grammar(d_s) {
        return Err(bad());
    }
    let n = parse_int(n_s, line)?;
    let d = parse_int(d_s, line)?;
    // §5.10 #12 rejects zero/negative notefracs — the fraction's VALUE:
    // `-1/-4` is a positive quarter and stays accepted.
    if n == 0 || d == 0 || (n < 0) != (d < 0) {
        return Err(err(
            line,
            format!("bad note fraction '{tok}' (must be positive)"),
        ));
    }
    let four = Rational::new(4, 1).expect("4/1");
    Rational::new(n, d)
        .and_then(|r| r.mul(four))
        .map_err(|e| err(line, e.msg().to_string()))
}

/// Note names (SPEC §5.6): `^([a-g])(#|b)?(-?\d)$`. Returns midi.
pub fn parse_note(tok: &str) -> Option<i64> {
    let b = tok.as_bytes();
    if b.len() < 2 || !(b'a'..=b'g').contains(&b[0]) {
        return None;
    }
    let semi_base: i64 = match b[0] {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return None,
    };
    // Try with an accidental consumed, else without (regex backtracking:
    // "b1" is letter b + octave 1; "bb1" is letter b + flat + octave 1).
    let try_octave = |rest: &[u8]| -> Option<i64> {
        match rest {
            [d] if d.is_ascii_digit() => Some(i64::from(d - b'0')),
            [b'-', d] if d.is_ascii_digit() => Some(-i64::from(d - b'0')),
            _ => None,
        }
    };
    let (acc, octave) = if b[1] == b'#' || b[1] == b'b' {
        match try_octave(&b[2..]) {
            Some(o) => (if b[1] == b'#' { 1 } else { -1 }, o),
            None => (0, try_octave(&b[1..])?),
        }
    } else {
        (0, try_octave(&b[1..])?)
    };
    // No wrap normalization: b#1 → midi 36, cb1 → midi 23.
    Some(12 * (octave + 1) + semi_base + acc)
}

/// Lane grammar (SPEC §5.5): tokens re-joined with single spaces,
/// matched against `^\[([^\]]*)\]\s*(.*)$`; `div=(\S+)` searched
/// anywhere in the tail (first position with a non-empty `\S+` capture
/// wins, as a regex engine would advance past `div=` followed by
/// whitespace); other tail garbage is silently ignored (§5.10 #7).
struct LaneParts {
    vals: Vec<String>,
    div: Option<String>,
}

fn parse_lane(toks: &[&str], line: usize) -> Result<LaneParts, Error> {
    let joined = toks[1..].join(" ");
    let shape_err = || {
        err(
            line,
            "lane must be [v1 v2 ...] (optionally div=1/8)".to_string(),
        )
    };
    if !joined.starts_with('[') {
        return Err(shape_err());
    }
    let close = joined.find(']').ok_or_else(shape_err)?;
    let inside = &joined[1..close];
    let tail = &joined[close + 1..];
    let vals: Vec<String> = inside.split_whitespace().map(str::to_string).collect();
    if vals.is_empty() {
        return Err(err(line, "empty lane".to_string()));
    }
    let mut div: Option<String> = None;
    let mut from = 0;
    while let Some(pos) = tail[from..].find("div=") {
        let start = from + pos + 4;
        let rest = &tail[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end > 0 {
            div = Some(rest[..end].to_string());
            break;
        }
        from = start;
    }
    Ok(LaneParts { vals, div })
}

fn lane_div(parts: &LaneParts, clock: Rational, line: usize) -> Result<Rational, Error> {
    match &parts.div {
        Some(t) => parse_notefrac(t, line),
        None => Ok(clock), // the voice's CURRENT clock, bound at parse time
    }
}

/// `swing <amt>[%] [<notefrac>]` (SPEC §5.2/§5.4). The caller has
/// already handled `swing off`. A bare `swing` raises a clean
/// line-cited error (posture: semantics-probes §E6).
fn parse_swing(toks: &[&str], line: usize) -> Result<Swing, Error> {
    let amt_tok = toks.get(1).ok_or_else(|| {
        err(
            line,
            "swing needs an amount (e.g. swing 58% 1/16)".to_string(),
        )
    })?;
    let amount = parse_pct(amt_tok, line)?;
    if !(50.0..=80.0).contains(&amount) {
        return Err(err(line, format!("swing {amount} out of range 50..80")));
    }
    let sub = match toks.get(2) {
        Some(t) => parse_notefrac(t, line)?,
        None => Rational::new(1, 4).expect("1/4"), // sixteenth-note default
    };
    Ok(Swing { amount, sub })
}

pub fn parse(src: &str) -> Result<Score, Error> {
    let mut sc = Score {
        tempo: 120.0,
        bars: 1,
        seed: 1,
        swing: None,
        voices: Vec::new(),
    };

    for (idx, raw) in src.split('\n').enumerate() {
        let line = idx + 1; // 1-based, cited in every parse error
        let trimmed = raw.trim(); // Unicode whitespace, includes \r (CRLF ≡ LF)
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue; // comments are full-line only
        }
        let toks: Vec<&str> = trimmed.split_whitespace().collect();
        let kw = toks[0];
        let arg1 = |toks: &Vec<&str>| -> Result<String, Error> {
            toks.get(1)
                .map(|s| (*s).to_string())
                .ok_or_else(|| err(line, format!("'{kw}' needs an argument")))
        };
        match kw {
            // Positional quirk: tempo/bars/seed match anywhere, last wins.
            "tempo" => {
                let a = arg1(&toks)?;
                sc.tempo = parse_num(&a, &a, line)?;
            }
            "bars" => sc.bars = parse_int(&arg1(&toks)?, line)?,
            "seed" => sc.seed = parse_int_i128(&arg1(&toks)?, line)?,
            "swing" => {
                if let Some(v) = sc.voices.last_mut() {
                    // After the first voice this is a VOICE swing —
                    // silent, by design (SPEC §5.10 #2).
                    if toks.get(1) == Some(&"off") {
                        v.swing = VoiceSwing::Off;
                    } else {
                        v.swing = VoiceSwing::Set(parse_swing(&toks, line)?);
                    }
                } else {
                    // `swing off` at file level: "off" fails num! —
                    // "bad number 'off'" (only voices can opt out).
                    sc.swing = Some(parse_swing(&toks, line)?);
                }
            }
            "voice" => {
                let name = toks
                    .get(1)
                    .ok_or_else(|| err(line, "voice needs a name".to_string()))?;
                let mut v = Voice::new((*name).to_string());
                for opt in &toks[2..] {
                    match opt.split_once('=') {
                        Some(("sample", val)) => {
                            v.sample = Some(match val {
                                "kick" => SampleKind::Kick,
                                "snare" => SampleKind::Snare,
                                "hat" => SampleKind::Hat,
                                "clap" => SampleKind::Clap,
                                _ => {
                                    return Err(err(line, format!("unknown voice option '{opt}'")));
                                }
                            });
                        }
                        Some(("synth", val)) => {
                            if val != "pluck" {
                                return Err(err(line, format!("unknown voice option '{opt}'")));
                            }
                            v.synth_pluck = true;
                        }
                        Some(("gain", val)) => v.gain = parse_num(val, val, line)?,
                        Some(("pan", val)) => v.pan = parse_num(val, val, line)?,
                        _ => return Err(err(line, format!("unknown voice option '{opt}'"))),
                    }
                }
                if v.sample.is_none() && !v.synth_pluck {
                    return Err(err(line, "voice needs sample= or synth=".to_string()));
                }
                sc.voices.push(v);
            }
            "clock" | "gate" | "vel" | "pitch" | "time" | "hum" | "prob"
                if sc.voices.is_empty() =>
            {
                return Err(err(
                    line,
                    format!("'{kw}' before any voice (or unknown directive)"),
                ));
            }
            "clock" => {
                let a = arg1(&toks)?;
                let v = sc.voices.last_mut().expect("voice");
                v.clock = parse_notefrac(&a, line)?;
            }
            "gate" => {
                let joined: String = toks[1..].concat();
                if joined.is_empty() {
                    return Err(err(line, "empty gate".to_string()));
                }
                let mut gate = Vec::new();
                for c in joined.chars() {
                    gate.push(match c {
                        'x' => GateChar::Hit,
                        'X' => GateChar::Accent,
                        '.' => GateChar::Rest,
                        _ => return Err(err(line, format!("gate char '{c}' (use x X .)"))),
                    });
                }
                sc.voices.last_mut().expect("voice").gate = Some(gate);
            }
            "vel" => {
                let parts = parse_lane(&toks, line)?;
                let mut vals = Vec::new();
                for t in &parts.vals {
                    let v = parse_int(t, line)?;
                    if v < 0 {
                        // Decided posture (SPEC §5.10 #9): reject
                        // negative vel at parse (expected-to-diverge).
                        return Err(err(line, format!("negative vel {v}")));
                    }
                    vals.push(v);
                }
                let v = sc.voices.last_mut().expect("voice");
                let div = lane_div(&parts, v.clock, line)?;
                v.vel = Some(Lane { vals, div });
            }
            "pitch" => {
                let parts = parse_lane(&toks, line)?;
                let mut vals = Vec::new();
                for t in &parts.vals {
                    if parse_note(t).is_none() {
                        return Err(err(
                            line,
                            format!("bad note '{t}' (want e.g. e1, f#2, bb0)"),
                        ));
                    }
                    vals.push(t.clone()); // the name as written
                }
                let v = sc.voices.last_mut().expect("voice");
                let div = lane_div(&parts, v.clock, line)?;
                v.pitch = Some(Lane { vals, div });
            }
            "time" => {
                let parts = parse_lane(&toks, line)?;
                let mut vals = Vec::new();
                for t in &parts.vals {
                    if t.ends_with("ms") {
                        vals.push(TimeEntry::Ms(parse_ms(t, line)?));
                    } else {
                        vals.push(TimeEntry::Frac(parse_num(t, t, line)?));
                    }
                }
                let v = sc.voices.last_mut().expect("voice");
                let div = lane_div(&parts, v.clock, line)?;
                v.time = Some(Lane { vals, div });
            }
            "hum" => {
                let a = arg1(&toks)?;
                sc.voices.last_mut().expect("voice").hum_ms = parse_ms(&a, line)?;
            }
            "prob" => {
                let a = arg1(&toks)?;
                sc.voices.last_mut().expect("voice").prob = parse_num(&a, &a, line)?;
            }
            _ => {
                if sc.voices.is_empty() {
                    return Err(err(
                        line,
                        format!("'{kw}' before any voice (or unknown directive)"),
                    ));
                }
                return Err(err(line, format!("unknown voice line '{kw}'")));
            }
        }
    }
    Ok(sc)
}

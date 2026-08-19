# beatcode — behavioral specification

`bc` is an offline, deterministic music compiler and renderer: a
plain-text score in, byte-identical audio out, with the WAV's sha256
printed as the receipt. Everything in this repository is the reference
for building it: this spec defines the exact observable behavior, and
[`goldens/`](goldens/README.md) holds machine-checkable vectors for
every normative claim. [`PLAN.md`](PLAN.md) is the build plan.

**How to read this document.** *Normative* = behavior the
implementation must reproduce to pass the goldens (Class A below),
plus the determinism rules. *Informative* = error-message texts,
the reference kit recipes (Class B), and the transcript behaviors
cataloged in §5.10 — surfaces where this implementation is licensed
to differ, each with its decided posture. §12 is implementation
guidance (Rust), including the exact decimal-rounding algorithm the
pipeline depends on.

---

## 1 · System overview and the determinism model

### 1.1 Pipeline

One small program, zero dependencies, offline-first. Pure pipeline;
side effects only at the edges:

```
score.bc ──parse──▶ Score ──compile──▶ [Event] ──┬─▶ JSONL on stdout   (events)
                                                 └─▶ WAV + sha256      (render)
                                                        └─▶ play / loop
```

Timing pipeline inside compile, in **fixed order** (transforms do not
commute; the order is spec):

```
grid (exact rationals) → swing → time-lane → humanize → performed_s (f64, clamped ≥ 0)
```

### 1.2 Dual time

Every event carries **both clocks**: `grid` — exact rational beats,
the editable truth — and `performed_s` — seconds after the pipeline,
with each contribution itemized (`swing_ms`, `lane_ms`, `hum_ms`).
Rationals everywhere upstream; floats only at the performed edge.

### 1.3 The three equivalence classes

The most important framing for the build. Outputs split by
arithmetic class:

| Class | Surface | Arithmetic | Contract |
|---|---|---|---|
| **A** | `events` JSONL (all fields), PRNG values, parser accept/reject | IEEE-754 basic ops (`+ − × ÷`), integer ops, exact decimal rounding — **no transcendental functions anywhere** | **Byte-exact match to the goldens, on any platform.** `goldens/events/` and `goldens/prng-vectors.jsonl` are the test. |
| **B** | The reference kit's synth buffers and WAV bytes | Class A plus transcendentals (`sin`, `exp`, `pow`, `cos`) | **Reference characterization only** (recorded on one platform). This implementation's kit is its own design (§8) and is *not* expected to match. |
| **C** | This implementation's WAV sha256s | Its own transcendental-free render loop | **Bit-identical across machines** — the flagship. Double-render equality, then cross-platform equality, with the hashes committed (§11). |

Verified fact behind Class A: the entire compile path — swing, lane,
humanize, performed-time arithmetic, PRNG output conversion, decimal
rounding, JSONL formatting — uses only exactly-specified operations,
so event lists are cross-platform bit-exact by construction. Only
audio involves transcendentals, and there this implementation uses
pinned approximations (§8.4) so its audio is bit-exact across
machines too (Class C).

### 1.4 Determinism rules (binding)

No libm-backed float methods in the render path (`f64::sin/cos/exp/
tan/powf` lower to platform libm and vary by platform); no
`mul_add`/FMA; no fast-math; no threads in the render path; scalar or
fixed-order summation; **no `HashMap` anywhere that touches output
order** (iteration order is randomized per process); transcendentals
only at coefficient time via pinned polynomial approximations or
precomputed constants; pinned toolchain; zero crate dependencies;
hand-written SHA-256 (FIPS 180-4).

---

## 2 · Core data model

### 2.1 Score (file level)

| Field | Type | Default | Notes |
|---|---|---|---|
| `tempo` | f64 | `120.0` | beats per minute; `spb = 60.0 / tempo` (f64 division) |
| `bars` | int | `1` | pattern length in 4/4 bars; `pattern_beats = 4 × bars` (exact rational) |
| `seed` | int | `1` | drives every stochastic transform; masked to u64 two's-complement at use (§4.2) |
| `swing` | Option\<Swing\> | none | global swing; settable only before the first voice (§5.2) |
| `voices` | Vec\<Voice\> | `[]` | in file order |

`Swing = { amount: f64 (50.0..=80.0), sub: Rational /* beats */ }`.

### 2.2 Voice

| Field | Type | Default | Set by |
|---|---|---|---|
| `name` | String | — | `voice <name> …` (any token; no validation, duplicates allowed) |
| `sample` | Option\<String\> | none | `sample=kick\|snare\|hat\|clap` |
| `synth` | Option\<String\> | none | `synth=pluck` (only value) |
| `gain` | f64 | `1.0` | `gain=<num>` (unvalidated range) |
| `pan` | f64 | `0.0` | `pan=<num>` (−1 left … +1 right; unvalidated) |
| `clock` | Rational beats | `1/4` (= a **sixteenth note**) | `clock <notefrac>` |
| `gate` | Option\<Vec\<GateChar\>\> | none | `gate …` — required by compile |
| `vel` | Option\<Lane\<i64\>\> | none | `vel [..] [div=..]` |
| `pitch` | Option\<Lane\<Note\>\> | none | `pitch [..] [div=..]` |
| `time` | Option\<Lane\<TimeEntry\>\> | none | `time [..] [div=..]` |
| `hum_ms` | f64 | `0.0` | `hum <num>[ms]` |
| `prob` | f64 | `1.0` | `prob <num>` |
| `swing` | VoiceSwing | `Inherit` | `swing off` → `Off`; `swing <amt>[%] [frac]` → `Set(Swing)` |

`GateChar ∈ { x (hit), X (accent), . (rest) }`.
`TimeEntry = Ms(f64) | Frac(f64)`. `Note = { name: String, freq: f64 }`.
`Lane<T> = { vals: Vec<T> (non-empty), div: Rational beats }` — a
lane's `div` defaults to the voice's **current** `clock` at the moment
the lane line is parsed (order matters: a `clock` line after a `vel`
line does not retroactively change that lane's default div).

**VoiceSwing is genuinely three-state**: `Inherit` (use the score's
global swing, if any), `Off` (`swing off` — no swing even if a global
exists), `Set(Swing)` (override). A two-state `Option` cannot
represent this.

Voice kind at compile: `sample` if set, else pluck — **if both are
given, `sample` wins**. At least one is required (parse error
otherwise).

### 2.3 Event (the keystone contract)

One record per surviving (gate ∧ prob) step, fields exactly:

| Field | Type | Semantics |
|---|---|---|
| `voice` | String | voice name as written |
| `kind` | Sample name or pluck | serialized as `"kick"/"snare"/"hat"/"clap"` or the literal `"pluck"` (frequency is *not* serialized) |
| `step` | int | the voice-local step index `i` |
| `grid` | String | reduced rational beats, `"num/den"` (den always printed, incl. `"0/1"`, `"7/2"`) |
| `vel` | int | after accent processing (§6.4) |
| `swing_ms` | f64 | swing contribution × 1000, rounded to 3 decimals (§6.10) |
| `lane_ms` | f64 | time-lane contribution × 1000, rounded to 3 decimals |
| `hum_ms` | f64 | humanize contribution × 1000, rounded to 3 decimals |
| `performed_s` | f64 | `max(0.0, grid_beats×spb + swing_s + lane_s + hum_s)` rounded to **6** decimals |
| `pitch` | Option\<String\> | present **iff the voice has a pitch lane, whatever its kind** (a sample voice with a pitch lane serializes `pitch` too — probed: `goldens/semantics-probes.txt` §E2); the value is the note name **as written** (`"a#1"`, `"bb1"`) at the event's grid position |
| `gain` | f64 | voice gain verbatim |
| `pan` | f64 | voice pan verbatim |

**The rounded values are the event.** The rounded `performed_s` is
stored in the event and used for *both* the sort key and sample
placement (§9.1). Round first, then sort/place — never carry
unrounded values further.

Sort: ascending by `(performed_s, voice, step)`; `voice` compares as
raw bytes; ties beyond the triple (possible with duplicate voice
names) keep score order — **use a stable sort**. `performed_s` is
finite and ≥ 0, so `partial_cmp().unwrap()` is safe.

---

## 3 · Exact rational time

Representation `{num: int, den: int}` with invariants: always reduced
by `gcd(|num|, |den|)`, `den > 0` (sign carried by `num`), and
`gcd(0, d) = |d|` so `0/anything` normalizes to `0/1`. A zero
denominator is rejected (§5.10 #12).

Operations (each result re-normalized through the constructor):

```
new(n, d):  g = gcd(n, d)   [non-negative gcd; gcd(0,d) = |d|]
            (n, d) = (n/g, d/g);  if d < 0 then (-n, -d)
add(a/b, c/d)  = new(a·d + c·b, b·d)
mul(a/b, c/d)  = new(a·c, b·d)
divr(a/b, c/d) = new(a·d, b·c)
to_f(a/b)      = a / b                    (single f64 division of exact ints)
floor_i(a/b)   = floor division a div b   (toward −∞: floor_i(-1/4) = -1)
int?(a/b)      = (b == 1)
to_s(a/b)      = "a/b"                    (both printed, reduced form)
```

Uses: grid positions (`clock × i`), lane indexing, swing parity, step
counts. All values non-negative in valid scores. `to_f` is the
**only** rational→float edge; it is correctly rounded, hence Class A.

**Width**: `i64` with `checked_*` arithmetic surfacing a clean
"rational overflow" error. Practical magnitudes are tiny
(numerators/denominators ≲ 10³ for real scores).

---

## 4 · The PRNG — byte-exact required

Pinned, hand-rolled, **keyed not sequential**: every value is derived
from *what it is for*, so edits elsewhere in a score never reshuffle
unrelated jitter. This is a design thesis (edit-stability of feel).
Nothing may depend on any library RNG.

### 4.1 fnv-1a (64-bit)

Over the UTF-8 **bytes** of the input string:

```
h = 0xCBF29CE484222325                    // offset basis = fnv("")
for each byte b:  h = ((h XOR b) × 0x100000001B3) mod 2^64
```

Golden: `fnv("") = 14695981039346656037`, `fnv("kick") =
17268634781200901759`, non-ASCII included (`"é"` hashes its two UTF-8
bytes) — 19 vectors in `goldens/prng-vectors.jsonl`.

### 4.2 The key chain (string-mixed — the subtle part)

`flt(seed, parts)` derives a u64 key by folding the parts through fnv
**via decimal string formatting**:

```
acc = seed masked to u64 two's-complement    // −1 → 18446744073709551615; 2^64 → 0
for each part p in parts:
    acc = fnv( utf8_bytes( format!("{p}|{acc}") ) )
```

where `p` renders as: strings verbatim (no quotes), integers in
decimal (with `-` if negative), and `acc` always in **unsigned
decimal**. Parts in practice are only strings and small non-negative
ints, but the formats above are normative. The golden vectors include
the expected `key` (final acc) for every case, so the chain can be
validated independently of splitmix.

### 4.3 splitmix64 finalizer

```
s = (key + 0x9E3779B97F4A7C15) mod 2^64
z = ((s XOR (s >> 30)) × 0xBF58476D1CE4E5B9) mod 2^64
z = ((z XOR (z >> 27)) × 0x94D049BB133111EB) mod 2^64
out = z XOR (z >> 31)
```

(Standard splitmix64, applied to the key once — a one-shot finalizer,
not a sequential generator.)

### 4.4 u64 → f64 — the range includes 1.0

```
flt = (out as f64) / 18446744073709551616.0      // ÷ 2^64
```

The u64→f64 conversion **rounds to nearest (ties even)** — Rust's
`as f64` does exactly this. Consequence: outputs ≥ 2^64 − 2^10 round
up to 2^64, so **`flt` returns values in `[0.0, 1.0]` inclusive**.
Golden: `18446744073709551615 / 2^64 = 1.0`. Downstream this is
benign (`prob` uses strict `<`; humanize maps 1.0 to a full +hum_ms
push) but an implementation that "fixes" it to `[0,1)` breaks
byte-equality.

### 4.5 Key inventory (all draws in the system)

| Draw | Call | When |
|---|---|---|
| gate probability | `flt(seed, [voice_name, "prob", step_i])` | only when `prob < 1.0` (short-circuit; keyed, so evaluation order is irrelevant anyway) |
| humanize | `flt(seed, [voice_name, "hum", step_i])` | only when `hum_ms > 0.0` |
| synth noise | `flt(fnv("sample\|" ++ tag), [i])` | per noise sample `i`; **independent of the score seed** — the kit is a constant |

Duplicate voice names collide on purpose-level keys: two voices named
`k` with `hum 3` get *identical* jitter per step. Not a bug; a score
semantics fact.

### 4.6 Noise streams

`noise(tag, n) = [ flt(fnv("sample|" <> tag), [i]) × 2.0 − 1.0  for i in 0..n−1 ]`
— values in `[−1.0, 1.0]`, dependent only on `(tag, i)`, so a prefix
of a longer stream equals a shorter stream (keyed property). Tags
used: `"kick-click"`, `"snare"`, `"hat"`, `"clap"`. First-8 goldens
for each in `prng-vectors.jsonl`.

### 4.7 Validation

`goldens/prng-vectors.jsonl`: 87 vectors — fnv strings, `flt` over a
grid of seeds (incl. negative, 2^64−1, > 2^64) × part shapes, with
intermediate `key` u64s, floats printed shortest-round-trip (parse
and compare **bit patterns**).

---

## 5 · Score format

### 5.1 Lexical structure

- Line-oriented. Split input on `"\n"`. Each line is trimmed
  (Unicode whitespace, which includes `\r` — **CRLF files parse
  identically to LF files**; probed, `goldens/semantics-probes.txt`
  §E4). Lone-CR (classic Mac) line endings are not supported (the
  file collapses into one whitespace-separated line).
- After trimming: empty → skip; first character `#` → comment, skip.
  **Comments are full-line only** — `#` is a sharp in pitch names
  (`f#1`), so there are no inline comments; a trailing `# …` inside a
  `gate` line is a *gate character error*.
- Tokenization: split on Unicode-whitespace runs.
- Line numbers are 1-based and cited in every parse error.

### 5.2 File-level directives

| Directive | Grammar | Semantics |
|---|---|---|
| `tempo <num>` | num per §5.8 | sets global tempo. **Positional quirk: matches anywhere**, even after voices — last one wins for the whole score. Same for `bars`, `seed`. |
| `bars <int>` | int per §5.8 | pattern length in 4/4 bars; **`bars < 1` is rejected** (compile-time error; §5.10 #13) |
| `seed <int>` | int | PRNG seed (may be negative or huge; masked at use) |
| `swing <amt>[%] [<notefrac>]` | amt: num; trailing `%` suffix(es) trimmed (§5.8); sub defaults to `1/4` **beat** (= a sixteenth note, i.e. `swing 58%` ≡ `swing 58% 1/16`) | global swing. **Only matches while no voice has been declared**; after the first voice, a `swing` line configures the *most recent voice* instead (§5.4 — silent, by design). Amount validated: `50.0 ≤ a ≤ 80.0` else parse error. `swing off` at file level is a parse error (only voices can opt out). |

Extra tokens after the consumed ones are ignored (`tempo 88 bpm` is
fine; the annotated examples rely on this).

### 5.3 Voice declaration

```
voice <name> <opt>…      opt ::= sample=kick|snare|hat|clap | synth=pluck
                               | gain=<num> | pan=<num>
```

- `<name>`: the next token, verbatim; no character restrictions, no
  uniqueness check.
- Options split on the **first** `=`. Unknown keys, bare tokens, and
  invalid `sample`/`synth` values are line errors (`sample=tom`
  reports as *unknown voice option*).
- After options: at least one of `sample`/`synth` required, else
  error. Both present → `sample` wins at compile.
- Declaring a voice pushes a fresh voice with the §2.2 defaults; all
  subsequent recognized voice lines mutate the most recent voice.

### 5.4 Voice lines (only valid after at least one voice)

| Line | Grammar | Semantics |
|---|---|---|
| `clock <notefrac>` | §5.7 | step size in beats. Affects the default `div` only of lanes parsed *after* it. |
| `gate <chars>…` | tokens joined with **no separator**, then split into codepoints; each must be `x`/`X`/`.`; empty → error | so `gate x.x. x.x.` (spaces for readability) is an 8-step gate. Repeated `gate` lines: last wins (all voice lines overwrite). |
| `vel [v…] [div=f]` | lane (§5.5) of ints | velocity lane; **negative values are rejected at parse** (§5.10 #9) |
| `pitch [n…] [div=f]` | lane of notes (§5.6) | tonal lane (meaningful for pluck voices; parseable on any voice — and serialized for any voice, §2.3) |
| `time [t…] [div=f]` | lane of time entries: token ending in `ms` → `Ms(num-before-ms)`; otherwise → `Frac(num)` | per-step microtiming: `+8ms`/`-6ms` absolute, `-0.02`/`+0.25` as a fraction of the **voice's** step length |
| `hum <num>[ms]` | trailing `ms` suffix(es) trimmed (§5.8) | humanize amplitude in ±ms; `≤ 0` is inert (no draw, no field contribution) |
| `prob <num>` | | per-step keep probability; `≥ 1.0` → no draw; `≤ 0` drops everything; unvalidated range |
| `swing off` | | this voice ignores swing entirely |
| `swing <amt>[%] [<notefrac>]` | same grammar/validation as global | per-voice override |
| anything else | | line error `unknown voice line '<kw>'` |

### 5.5 Lane grammar (shared)

Tokens after the keyword are re-joined with single spaces and matched
against `^\[([^\]]*)\]\s*(.*)$`:

- Inside the brackets: whitespace-separated values, parsed by the
  per-lane value parser; empty list → error.
- In the tail: `div=(\S+)` searched anywhere; present → notefrac
  (§5.7); absent → the voice's current `clock`. Other tail garbage is
  silently ignored (§5.10 #7). `[100]div=1/8` (no space) parses fine.

### 5.6 Note names

Regex `^([a-g])(#|b)?(-?\d)$` (lowercase letter; single-digit octave,
optionally negative — `e10` is invalid, `a-1` is valid):

```
semis: c=0 d=2 e=4 f=5 g=7 a=9 b=11;  # adds 1, b subtracts 1
midi = 12 × (octave + 1) + semi           // e1 → 28, a4 → 69
freq = 440.0 × 2^((midi − 69) / 12)
```

No wrap normalization: `b#1` → midi 36, `cb1` → midi 23. The event's
`pitch` field carries the *name as written*; `freq` matters only to
synthesis, so Class A is unaffected. Since `midi − 69` is always an
integer, compute `freq = 440.0 × r^(midi−69)` with the pinned
constant `r = 1.0594630943592953` (2^(1/12)) and an integer-exponent
loop — no transcendental call (§8.4).

### 5.7 Note-value fractions → beats

`<int>/<int>` denotes a fraction **of a whole note**, converted once
at parse into rational beats (4/4): `beats = new(4 × n, d)` — so
`1/16` → `1/4` beat, `1/8` → `1/2`, `1/4` → `1`, `1/1` → `4`, `1/12`
→ `1/3` (triplet sixteenths). Anything not matching `n/d` with
parseable ints is a line error. **Zero or negative notefracs are
rejected** with a clean error (§5.10 #12).

### 5.8 Numbers

- `num!` (floats): strip **all leading `+`** characters, then parse
  requiring full consumption of the grammar
  `sign? digits ('.' digits)? ([eE] sign? digits)?`. Accepts `58`,
  `+8`, `-0.02`, `4.5`, `++5` (via the leading-`+` strip), and
  scientific notation `1e3`/`1.0e3`. Rejects `.5`, `58.`, `1_000`,
  `0x10`. (Transcript: `goldens/float-semantics.txt`.)
- `int!` (integers): decimal integer, full consumption. One leading
  sign (`+`/`-`) is accepted (`bars +2` works); no extra `+`
  stripping, so `++2` fails where `num!` would take `++2.0`'s float
  cousin. `seed` may exceed 64 bits (§12.6).
- `ms!`: trailing `"ms"` **whole-suffix repetitions** trimmed, then
  num!: `4.5msms` → 4.5, but `4.5mss` and `4.5sm` are rejected
  (probed — `goldens/semantics-probes.txt` §E5).
- swing amount: trailing `"%"` characters trimmed (single-char
  suffix, so `58%%` → 58.0), then num!.

### 5.9 Error model

- Parse errors: `score error, line N: <message>` — the line number is
  normative, the message text informative. (One exception recorded in
  the transcripts: a bare `swing` with no arguments crashes uncleanly
  there — this implementation raises a clean line-cited error instead;
  `goldens/semantics-probes.txt` §E6.)
- Compile-time errors (no line numbers): missing gate (`voice X has
  no gate`), non-dividing clock (`voice X: clock does not divide
  pattern`), and the rejections adopted in §5.10.
- The CLI prints clean errors and exits non-zero everywhere — except
  `loop` mode, which prints `!! <message> (fix and save again)` and
  keeps watching (§10).
- The accept/reject probe transcript is
  `goldens/parser-behaviors.txt` — **42 cases**; treat *which* inputs
  succeed/fail (and the cited line) as the contract, with the two
  §5.10 exceptions below.

### 5.10 Transcript-behavior catalog and decided postures

The golden transcripts record these behaviors; each row states what
**this implementation does**. "Match" = reproduce exactly.
Two rows diverge from the probe transcript on purpose; those probe
cases are **expected-to-diverge** in the conformance run.

| # | Behavior | Posture |
|---|---|---|
| 1 | `tempo`/`bars`/`seed` accepted after voices, last-wins globally | Match. |
| 2 | `swing` after the first voice silently becomes a *voice* swing on the most recent voice | Match (silent). |
| 3 | Repeated voice lines (e.g. two `gate`s): last wins, silently | Match. |
| 4 | Duplicate voice names allowed; PRNG keys collide (identical jitter) | Match — observable score semantics. |
| 5 | `sample=` and `synth=` both present: sample wins | Match. |
| 6 | Scientific notation in num! (`tempo 1e2`) | Match. |
| 7 | Lane tail garbage ignored (`vel [100] foo`); first `div=` match wins | Match. |
| 8 | Trailing tokens after fixed-arity directives ignored (`tempo 88 bpm`) | Match — the examples rely on it. |
| 9 | `vel` values unvalidated: > 127 passes through (plain hits); negative values only crashed later at render | **Reject negative vel at parse** — the transcript's "negative vel parses" case is expected-to-diverge. Values > 127 still pass through. |
| 10 | `prob`/`gain`/`pan` ranges unvalidated (pan beyond ±1 phase-flips a channel via the pan law) | Match (unvalidated). |
| 11 | `tempo 0` crashed with a bare arithmetic error; negative tempo yields all-zero performed times | **Reject `tempo == 0` with a clean error** (the transcript case also errors — accept/reject matches). **Negative tempo stays accepted** (the transcript shows it OK; all `performed_s` clamp to 0.0). |
| 12 | Zero-denominator or zero notefracs crashed the rational constructor | **Reject zero/negative notefracs cleanly** (the transcript case also errors — accept/reject matches). |
| 13 | `bars 0` emitted phantom steps `0` and `−1`, the latter indexing the gate from the end | **Reject `bars < 1`** — the transcript's `bars 0` OK-case is expected-to-diverge. |
| 14 | Unicode whitespace tokenizes; trimming is Unicode | Match. |
| 15 | CRLF line endings | Match — CRLF already parses identically to LF (§5.1; probed §E4). |
| 16 | Repeated whole-suffix `ms`/`%` trimming (`58%%`, `4.5msms`) | Match (§5.8). |

---

## 6 · Compilation to events

### 6.1 Pattern length and step counts

```
spb           = 60.0 / score.tempo                      // f64
pattern_beats = R(4 × bars, 1)
per voice:
  n_steps_r   = pattern_beats ÷ clock                   // rational divr
  require int?(n_steps_r)  else compile error ("clock does not divide pattern")
  n_steps     = floor_i(n_steps_r)
  step_s      = to_f(clock) × spb                       // seconds per step, f64
  eff_swing   = voice.swing == Inherit ? score.swing : voice-resolved
                // Inherit → global (may be none) · Off → none · Set(s) → s
```

The **gate length does not need to divide** `n_steps` — lanes and
gate cycle at their own lengths (that is the point: `poly.bc` is
built on a 12-char gate over 16-step bars).

### 6.2 Per-step algorithm (exact order)

For `i in 0 .. n_steps−1`:

```
ch        = gate[i mod gate_len]
keep_gate = ch ∈ {x, X}
keep_prob = prob ≥ 1.0  ||  flt(seed, [name, "prob", i]) < prob
if !(keep_gate && keep_prob): no event
grid      = clock × R(i,1)                              // exact rational beats
vel0      = lane_val(vel_lane, grid, default 100)
vel       = ch == X ? min(127, round(vel0 × 1.15)) : vel0    // round: ties away from zero
swing_s   = swing_offset(eff_swing, grid, spb)          // §6.5, f64 seconds
lane_s    = time_offset(time_lane, grid, step_s)        // §6.6
hum_s     = hum_ms > 0 ? (2·flt(seed,[name,"hum",i]) − 1) × hum_ms / 1000 : 0.0
pitch     = pitch_lane? lane_val(pitch_lane, grid, none) : none
performed = max(0.0, to_f(grid) × spb + swing_s + lane_s + hum_s)
```

**Every f64 expression above is evaluated in strict left-to-right
association — no reordering, no FMA.** For `performed` that means
`(((to_f(grid) × spb) + swing_s) + lane_s) + hum_s`; for `hum_s`,
`((2·flt − 1) × hum_ms) / 1000`; for `pair_s` in §6.5,
`(2.0 × to_f(sub)) × spb`. This order is normative — it is what makes
byte-exactness achievable.

### 6.3 Lane indexing — time-indexed, not event-indexed

```
lane_val(lane, grid) = lane.vals[ floor_i(grid ÷ lane.div)  mod  len(lane.vals) ]
```

(`÷` is rational divr; the quotient is ≥ 0 for all valid scores so
truncating vs. flooring `mod` never differs.) The lane value depends
on **grid time**, not on how many events fired before — a 4-entry vel
lane at `div=1/8` patterns accents identically whether the gate is
dense or sparse. This four-line function *is* the sequencing model;
mismatched `len × div` against the pattern is where polymeter and
evolving variation come from.

### 6.4 Velocity and accent

Default 100 when no vel lane. `X` accent: `min(127, round(v × 1.15))`
with round-half-**away** (`115 → 132.25 → 132 → 127`; `60 → 69`).
Plain `x` applies the lane value unmodified (no upper clamp).

### 6.5 Swing (MPC convention)

```
swing_offset(none, …) = 0.0
swing_offset({amount, sub}, grid, spb):
    q = grid ÷ sub                      // rational
    if int?(q) and floor_i(q) is odd:
        pair_s = 2.0 × to_f(sub) × spb          // left-to-right, §6.2
        return (amount/100.0 − 0.5) × pair_s
    else return 0.0
```

Only events landing **exactly on odd integer multiples** of the
subdivision are delayed; everything else (even multiples, off-grid
positions) is untouched. 50 = straight (identity), 66⅔ ≈ triplet,
upper bound 80. Worked check (`examples/dilla.bc`): 58% at 88 bpm,
sub 1/16-note = 1/4 beat → `pair = 2 × 0.25 × 60/88 = 0.340909… s`;
delay `= 0.08 × pair = 0.027272… s` → `swing_ms 27.273` on every odd
sixteenth — visible throughout `goldens/events/dilla.events.jsonl`.

Per-voice resolution: `Inherit` → global (if any), `Off` → none,
`Set` → the override. This per-lane feel independence is the point:
dilla.bc layers a pushed kick, +14 ms dragged snare, humanized hats,
and an unswung bass against one global 58%.

### 6.6 Time-lane offsets

```
time_offset(none, …) = 0.0
time_offset(lane, grid, step_s):
    match lane_val(lane, grid):
      Ms(v)   → v / 1000.0              // absolute milliseconds
      Frac(v) → v × step_s              // fraction of THIS VOICE's step length
```

Both units exist because real feel has a tempo-relative and an
absolute component. A one-entry lane is a constant push/drag
(`time [+14ms]`). Negative totals are legal; the final clamp (§6.2)
floors at zero.

### 6.7 Humanize

Uniform in `[−hum_ms, +hum_ms]` (endpoints reachable, §4.4), seeded
and keyed per `(voice, "hum", step)` — same seed ⇒ same jitter; a new
seed is a new (reproducible) performance. Inactive unless
`hum_ms > 0.0`.

### 6.8 Probability

Keyed draw per `(voice, "prob", step)` compared with strict `<`. The
short-circuit at `prob ≥ 1.0` matters only for avoiding the draw;
keyed PRNG makes draw *order* irrelevant everywhere.

### 6.9 Performed time

Clamped at zero: an event pushed before t=0 (e.g. dilla's kick step 0
with a −4 ms lane) lands exactly at `0.0`. `max(0.0, x)` returns +0.0
for x = −0.0.

### 6.10 The decimal rounding rule (normative)

`swing_ms`/`lane_ms`/`hum_ms` = seconds × 1000 rounded to **3**
decimals; `performed_s` rounded to **6**; (CLI: WAV seconds to
**2**). The rule:

> **Round the *exact binary value* of the f64 to N decimal digits; on
> exact ties, round half away from zero. The result is the f64
> nearest to the rounded decimal. Sign of zero: when a negative
> non-zero value rounds to zero, the result is +0.0; −0.0 results
> only from an exactly −0.0 input** (probed:
> `goldens/semantics-probes.txt` §E1 — reachable through ordinary
> humanize/time draws, and `time [-0ms]` really does produce
> `"lane_ms":-0.0`).

- Exact-binary basis: `round(2.675, 2) = 2.67` (the double is
  2.67499999999999982…), `round(0.285, 2) = 0.28`, `round(1.005, 2)
  = 1.0` — never the "school" answer off the decimal literal.
- Ties are *reachable* (dyadic rationals): `round(0.125, 2) = 0.13`,
  `round(0.0625, 3) = 0.063`, `round(0.0078125, 6) = 0.007813`,
  negative mirror `−0.13`, `−0.007813`.
- Beware: Rust's `format!("{:.N}")` rounds ties **to even** — it
  disagrees at exactly these dyadic points. Use the exact algorithm
  in §12.5 for both value-rounding and formatting.
- **One known formatter boundary** (probed, §E3): the formatter that
  produced the transcripts double-rounds through an f64 multiply, so
  at last-ulp-near-half inputs it disagrees with the exact rule —
  `goldens/float-semantics.txt` shows `ftb(5.0e-7) = 0.000001` where
  the exact rule gives `0.0`. This implementation uses the exact rule
  everywhere; that one transcript line is **expected-to-diverge**.
  It cannot affect event bytes for the ms/performed fields (they are
  value-rounded first, and format-after-round is idempotent); only a
  pathological verbatim `gain`/`pan` literal (e.g. `gain=0.0000005`)
  could reach the boundary.

### 6.11 Sorting

By `(performed_s, voice_bytes, step)` ascending, stable (§2.3). This
total order is what makes compile output byte-stable, and it is also
the **mix accumulation order** (§9.3), so it is doubly normative.

### 6.12 Worked micro-examples (all present in the goldens)

From `examples/edge.bc` (tempo 90 → spb = 2/3 s; global swing 60%
1/8):

- **Clamp**: kick step 0 — lane `−6 ms`, hum `+0.462 ms` → raw
  `−0.005538 s` → `performed_s 0.0` with `lane_ms −6.0`,
  `hum_ms 0.462` still itemized.
- **Inherited swing + frac lane**: kick step 2 — grid `1/2` beat;
  q = (1/2)/(1/2) = 1 odd → `swing_ms 66.667` (= 0.1 × 2 × 0.5 × 2/3);
  time entry `+0.25` × step 1/6 s → `lane_ms 41.667`; + hum 0.937 →
  `performed_s 0.442604` (0.333333… + 0.066667 + 0.041667 + 0.000937).
- **Default-sub swing**: clap (voice `c`, `swing 55%`) step 1 — sub
  defaults to 1/4 beat; q = 1 odd → `swing_ms 16.667`
  (= 0.05 × 2 × 0.25 × 2/3).
- **Accent cap**: snare step 4 (`X`, vel lane 115) → `vel 127`;
  step 7 (`x`) → `vel 115`; steps 7/15 odd-q under `swing 75% 1/8` →
  `swing_ms 166.667`.
- **Alphabetical tiebreak at t=0**: voices `d`, `h`, `k` all at
  `performed_s 0.0` sort `d < h < k`.

---

## 7 · JSONL encoding

Hand-rolled, flat-map-only, deterministic:

- Drop every key whose value is None (in practice: only `pitch`).
- Sort keys as strings, byte order — the fixed full sequence is:
  `gain, grid, hum_ms, kind, lane_ms, pan, performed_s, pitch, step,
  swing_ms, vel, voice`.
- One object per line: `{"k":v,…}` — no spaces, `\n` after each line.
- **Integers**: plain decimal (`100`, `-50`).
- **Floats**: format the exact binary value at 6 decimals with the
  §6.10 rule, then *compact*: trim trailing zeros but always keep at
  least one fractional digit; keep the sign — **including on zero**
  (a stored `-0.0` formats as `"-0.0"`; per §6.10, ms fields hold
  `-0.0` only from an exactly `-0.0` input). Examples: `1.0`, `0.95`,
  `27.273000000000003 → "27.273"`, `5.04e-4 → "0.000504"`,
  `1.0e-7 → "0.0"`, `-0.0 → "-0.0"`, `100.0 → "100.0"`.
  (Because ms fields were already value-rounded at 3 decimals, the
  6-decimal format never adds digits to them; `performed_s` was
  value-rounded at 6, so for it format-vs-value rounding is
  idempotent.)
- **Strings**: `"` wrapped; escape only backslash (first) and double
  quote — no control-character or unicode escaping (raw UTF-8 passes
  through).
- **kind**: samples serialize as their name string; pluck serializes
  as the constant string `"pluck"` (frequency dropped).

The `events` CLI command prints events in sort order, one line each.
Byte-compare against `goldens/events/*.events.jsonl`.

---

## 8 · Synthesis — the kit (Class B/C)

44 100 Hz, mono, f64 buffers, procedurally synthesized, memoized. All
noise comes from §4.6 streams, so buffers are fully deterministic.
The recipes below are the **reference kit characterization** — this
implementation designs its own kit to these shapes with pinned math
(§8.4); matching the reference's audio byte-for-byte is a non-goal
(Class B), but the kit must be bit-stable across machines (Class C).

Common pattern: per-sample loop threading phase/filter state,
`t = i / 44100.0`. **Phase pre-increments**: `ph += 2π·f/sr` *before*
the sine is taken, so sample 0 already carries one increment.

| Voice | Length | Recipe (reference constants) |
|---|---|---|
| `kick` | `trunc(0.30·sr)` = 13230 | `f(t) = 44 + 76·e^(−t/0.040)` (120→44 Hz sweep); `ph += 2π f/sr`; `out = sin(ph)·e^(−t/0.115)·0.95 + click`, click = `noise("kick-click")[i]·0.35·(1 − i/40)` for `i < 40` else 0 |
| `snare` | `trunc(0.22·sr)` = 9702 | tone `sin(ph)·0.45·e^(−t/0.055)` at constant 186 Hz + noise `n[i]·0.80·e^(−t/0.080)` |
| `hat` | `trunc(0.075·sr)` = 3307 | one-pole highpass over noise: `y = 0.92·(y₁ + x − x₁)` (state init 0,0); `out = y·0.7·e^(−t/0.021)` |
| `clap` | `trunc(0.26·sr)` = 11466 | three bursts at samples `{0, 485, 1014}` (= trunc(0.011·sr), trunc(0.023·sr)), each window `[b, b+352)` adding `n[i]·0.75` (windows are disjoint but the *sum* form is the spec); plus tail for `t > 0.028`: `n[i]·0.55·e^(−(t−0.028)/0.065)` — tail and third burst overlap and add |
| `pluck(freq)` | `trunc(0.45·sr)` = 19845 | `ph += 2π·freq/sr`; `cyc = ph/2π`; `saw = 2(cyc − ⌊cyc⌋) − 1`; `raw = (saw·0.7 + sin(2·ph)·0.15)·e^(−t/0.150)`; one-pole lowpass `lp += 0.18·(raw − lp)`; output `lp` |

Memo cache: keyed `"kick"|"snare"|"hat"|"clap"` and
`(pluck, round(freq × 100))` — **0.01 Hz quantization** of the cache
key (round ties-away). A pluck voice with **no pitch lane**
synthesizes at `110.0` Hz. A cache is order-free (never iterated), so
any map type is fine.

### 8.4 Transcendental-free realization (binding)

The reference recipes use `sin`, `exp`, `pow`, `cos`. This
implementation replaces them so the render path is bit-exact across
machines:

- **Sine**: one pinned range-reduced odd-polynomial `sin` with
  coefficients documented in the source (accuracy ≈ 1e-9 is ample);
  `cos(x) = sin(x + π/2)` with π/2 as the f64 literal.
- **Envelopes** (`e^(−t/τ)`): multiplicative accumulation —
  `env *= k` per sample, with each `k = e^(−1/(sr·τ))` **precomputed
  and embedded as an f64 literal** with its derivation in a comment
  (the τ set is fixed by the recipes). Same for the kick sweep decay
  and the clap tail's starting value.
- **Velocity curve** `pow(x, 1.5)` = `x·sqrt(x)` — `sqrt` is an
  exactly-specified IEEE op, allowed.
- **Note frequency**: integer-exponent loop on the pinned `2^(1/12)`
  literal (§5.6).
- π is the f64 literal `std::f64::consts::PI` (a constant, not a
  libm call).

An automated banned-token check (`.sin(`, `.cos(`, `.exp(`, `powf`,
`mul_add` over `src/`) keeps the rule enforced.

---

## 9 · Mixdown and WAV

### 9.1 Placement

Per event, in **sorted event order** (§6.11):

```
f0 = trunc(performed_s × 44100.0)        // performed_s = the ROUNDED 6-decimal field
buffer b = synth(kind)                    // §8
for j in 0..len(b)−1:  frame[f0 + j] += (b[j]·aL, b[j]·aR)
```

`trunc` toward zero (values are ≥ 0 here). Using the *rounded*
`performed_s` quantizes placement to microseconds (≈ 0.044 frames) —
normative; do not place from an unrounded value.

### 9.2 Amplitude and pan

```
amp = pow(vel / 127.0, 1.5) × gain               // as x·sqrt(x), §8.4
ang = (pan + 1.0) × π/4                          // −1 → 0, 0 → π/4, +1 → π/2
aL  = amp × cos(ang);   aR = amp × sin(ang)      // constant-power pan, pinned sine
```

Equal-power at center (cos = sin = √2/2 ≈ 0.7071). Pan values beyond
±1 leave the first quadrant and phase-flip a channel (§5.10 #10).

### 9.3 Accumulation model

Mix into a dense stereo f64 buffer initialized to zero, adding each
event's samples in **sorted-event order, frame order within each
event's buffer**. Order is normative: float addition does not commute
in rounding, so overlapping events must be summed in exactly this
order. No reassociation, no SIMD/reduction reordering, no `mul_add`.

### 9.4 Track length

```
last  = highest frame index touched
frames = last + trunc(0.5 × 44100)  = last + 22050        // half-second tail
empty mix (zero events): frames = 44100                    // one second of silence
```

### 9.5 Peak normalization (conditional)

Scan all frames for `peak = max(|L|, |R|)` (order-free max). If
`peak > 0.98` (strict), every output sample is scaled by
`0.98 / peak`; else scale 1.0. The render receipt reports
`(peak-normalized)` when scaling occurred. (Of the committed scores,
only `edge.bc` triggers it — by design, `gain=1.1`.)

### 9.6 Quantization to s16

Per frame `f` in `0..frames−1` (untouched frames are silence):

```
s16(v) = round( clamp(v × norm, −1.0, 1.0) × 32767.0 )    // ties away from zero
```

Range is **symmetric ±32767** — the −32768 code is never produced.
Interleave L then R, each little-endian i16. (Rust's `f64::round` is
ties-away and exactly specified — use it.)

### 9.7 WAV container (exact bytes)

44-byte canonical PCM header followed by `frames × 4` data bytes:

```
"RIFF"  u32le(36 + data_size)  "WAVE"
"fmt "  u32le(16)  u16le(1 PCM)  u16le(2 ch)  u32le(44100)
        u32le(176400 byte-rate)  u16le(4 block-align)  u16le(16 bits)
"data"  u32le(data_size)                     data_size = frames × 4
```

Reference header hex (first 44 bytes of a 466 360-byte four.bc
render, i.e. 44 + 116579×4 — sizes depend on event content, the
*layout* is the golden):

```
52 49 46 46 b0 1d 07 00 57 41 56 45 66 6d 74 20
10 00 00 00 01 00 02 00 44 ac 00 00 10 b1 02 00
04 00 10 00 64 61 74 61 8c 1d 07 00
```

Output directory is created on demand.

### 9.8 The receipt

Every render computes sha256 over the **entire file bytes** and
prints lowercase hex — the determinism receipt. Reported seconds =
`frames / 44100` rounded to 2 decimals (§6.10 rule). This
implementation commits its own render hashes as goldens (§11).

---

## 10 · CLI

| Command | Behavior |
|---|---|
| `events <score.bc>` | compile, print JSONL to stdout (one event per line, sorted) |
| `render <score.bc> [out]` | render; default out `renders/<basename minus .bc>.wav`; print `"<path>  <s>s  <n> events  sha256=<hex>"` + `"  (peak-normalized)"` when scaled (two-space separators) |
| `play <score.bc>` | render to the default path, print `"<path>  <s>s  sha256=<first 12 hex>…"`, then play |
| `loop <score.bc>` | poll the file's mtime every 200 ms; on change re-render + play; **an error prints `!! <msg> (fix and save again)` and the loop keeps watching** (the jam must survive typos); a vanished file just keeps polling |
| `demo` | render every `examples/*.bc` (lexicographic order) |
| anything else | usage text |

Playback: first found of `afplay` → `paplay` → `aplay -q` →
`mpv --really-quiet` → `ffplay -nodisp -autoexit -loglevel quiet`;
none → print `no audio player found — rendered file is at <path>`.
Blocking (wait for the player to exit). All errors print cleanly and
exit non-zero (except inside `loop`, above).

---

## 11 · Goldens and acceptance

### 11.1 Property tests (port these)

1. compile determinism (two compiles byte-equal);
2. render determinism (two renders, equal sha256);
3. swing 50% is the identity on performed times;
4. swing 66% delays off-sixteenths and leaves downbeats untouched;
5. humanize stable under same seed, different under a new seed;
6. WAV header sanity (RIFF/WAVE bytes, §9.7 layout).

### 11.2 Golden files (see `goldens/README.md`)

| File | Class | Validates |
|---|---|---|
| `events/{four,dilla,poly,edge}.events.jsonl` | A | full-score event compilation, byte-exact (24/44/86/58 events) |
| `prng-vectors.jsonl` | A | fnv, key chain, splitmix output, noise streams (87 vectors) |
| `float-semantics.txt` | A¹ | rounding/format/parse rules (probe transcript) |
| `parser-behaviors.txt` | A² | 42 accept/reject cases with line numbers and event summaries |
| `semantics-probes.txt` | A | sign-of-zero, pitch serialization, CRLF, suffix, error-shape probes |

¹ two lines are **expected-to-diverge**: the `ftb(5.0e-7)` formatter
boundary (§6.10) and the final `pow` probe (transcendental —
platform-scoped, not Class A).
² two cases are **expected-to-diverge** by decided posture (§5.10):
`bars 0` and negative `vel`. Error *texts* in the transcript are
informative only; accept/reject and line numbers are the contract.

### 11.3 Acceptance — v0.1 is done when

1. `cargo fmt --check` + `cargo clippy` clean; zero dependencies;
   pinned toolchain (`rust-toolchain.toml`).
2. PRNG byte-exact against all 87 `prng-vectors.jsonl` entries.
3. **Class A**: `bc events` byte-equals all four
   `goldens/events/*.events.jsonl` files.
4. Parser conformance against `parser-behaviors.txt`: accept/reject
   + cited line numbers match on 40/42, with exactly the two §11.2
   expected-to-diverge cases.
5. The six §11.1 properties green.
6. sha256 green on FIPS 180-4 standard vectors; WAV header bytes
   match §9.7.
7. **Class C**: double-render hash equality *and* cross-machine hash
   equality (CI matrix); this implementation's render hashes for the
   four example scores committed to `goldens/` with the platforms
   recorded.
8. `bc render examples/dilla.bc` renders and `play`s; `loop`
   survives a score error and keeps watching.

---

## 12 · Implementation guidance (Rust)

### 12.1 Engineering defaults

Zero crate dependencies; pinned toolchain; plain Rust — enums +
exhaustive `match`, owned values (clone freely; optimize only with
profiler evidence), minimal generics, no macros beyond derive, no
async. Lib/bin split optional; golden checks as `cargo test`
integration tests.

### 12.2 Module map (suggested)

| Module | Job | Spec | Watch for |
|---|---|---|---|
| `main.rs` | CLI dispatch | §10 | clean errors + exit codes |
| `rational.rs` | exact beats | §3 | gcd(0,d), sign normalization, checked overflow |
| `prng.rs` | fnv + chain + splitmix | §4 | decimal-string mixing; u64→f64 includes 1.0 |
| `score.rs` | parser + error enum | §5 | three-state swing; lane default-div timing; positional directives |
| `events.rs` | §6 pipeline | §6 | operation order; rounded values are the event |
| `jsonl.rs` | encoder | §7 | §6.10 rounding; signed zero; minimal escaping |
| `decfmt.rs` | §12.5 rounding/formatting | §6.10, §7 | the E1 sign-of-zero branch |
| `synth.rs` | pinned-math kit | §8 | no libm; document coefficients |
| `render.rs` | place, mix, normalize, s16 | §9 | event-order summation; rounded placement; strict > 0.98 |
| `wav.rs` | header + write | §9.7 | byte-exact 44-byte header |
| `sha256.rs` | FIPS 180-4 by hand | — | standard test vectors |

### 12.3 Type sketches (spirit, not prescription)

- `enum Kind { Kick, Snare, Hat, Clap, Pluck }` with the optional
  frequency carried beside it — JSONL prints names / `"pluck"`.
- `enum VoiceSwing { Inherit, Off, Set(Swing) }` — see §2.2.
- `enum TimeEntry { Ms(f64), Frac(f64) }`.
- `struct Lane<T> { vals: Vec<T>, div: Rational }`.
- A `ScoreError { line: u32, msg: String }` for parse errors;
  compile-error variants may carry the voice name (§5.9).
- Event as a struct with the **rounded** f64 fields (§2.3).

### 12.4 The seven traps (each has bitten a port like this before)

1. **`HashMap` iteration order** — randomized per process;
   same-machine double-render breaks. `Vec` + sort or `BTreeMap`
   anywhere order can reach output. (Memo caches that are only
   *looked up* are exempt.)
2. **Rounding ties** — Rust `{:.N}` formatting ties-to-even; the
   spec ties-away on the exact binary value. Dyadic ties are
   reachable (§6.10). Use §12.5.
3. **`u64 as f64` semantics** — correct (round-nearest-even) in
   Rust; do not "improve" the `[0,1]`-inclusive range (§4.4).
4. **Placement from rounded time** — `f0 = trunc(rounded_performed ×
   44100.0)`, not from the unrounded sum (§9.1).
5. **Summation order** — sorted-event order into the frame buffer;
   no reassociation, no SIMD/reduction reordering, no `mul_add`
   (§9.3).
6. **f64 sort keys** — `performed_s` is finite ≥ 0 here, so
   `partial_cmp().unwrap()` is fine, but keep the sort **stable**
   (ties from duplicate voice names preserve score order).
7. **String formatting inside the PRNG key chain** — the accumulator
   renders as *unsigned decimal* between fnv rounds (§4.2);
   formatting it signed, hex, or padded silently changes every draw.

### 12.5 Exact decimal rounding/formatting, zero-dep (≈40 lines)

One routine serves §6.10 value-rounding (n = 3, 6, 2) and §7
formatting (n = 6 + compact trim). Domain: finite `x` with
**|x| ≤ 2^53 / 10^n** (all pipeline values qualify by orders of
magnitude; outside that domain the final conversion is no longer
exact — assert or error there):

```
decompose: x = sign · m · 2^e   with 2^52 ≤ m < 2^53 (normal),  e ≤ 0
           (subnormals: smaller m; the domain guarantees e ≤ 0)
N  = m · 10^n                      // u128: 2^53 · 10^6 < 2^73, safe
k  = −e                            // 0 ..= 1074
q  = k < 128 ? N >> k : 0          // integer part of |x|·10^n
r≥½? : k == 0        → false
       k ≤ 127       → (N >> (k−1)) & 1 == 1   // top dropped bit
       k == 128      → N ≥ 2^127
       k ≥ 129       → false                    // value too small
q' = q + (r≥½ ? 1 : 0)             // ties (r exactly ½ has top bit 1, rest 0 —
                                   //  covered by the same test) → away from zero

value-rounding result:
    if q' == 0:  +0.0 when x is negative non-zero;  −0.0 only when x is −0.0
    else:        sign · (q' as f64) / (10^n as f64)   // exact operands, one
                                                      // correctly-rounded division
formatting result:      digits of q' zero-padded to n+1, split n from the right,
                        insert '.', trim trailing '0's keeping ≥ 1 fractional
                        digit, prepend '-' iff the f64 being formatted is
                        negative (a stored −0.0 formats as "-0.0"; §7)
```

The top-dropped-bit test implements round-half-away exactly: `r = ½`
has the top bit set (round up — away), `r > ½` likewise, `r < ½`
clear. Validate against `goldens/float-semantics.txt` (minus its two
expected-divergent lines, §11.2) plus the ms/performed fields of all
event goldens (which exercise it ~1500 times).

### 12.6 Numeric-width postures

- Rationals: `i64` + checked ops (§3), erroring on overflow.
- `seed`: parse as `i128` (accepts negatives and > 2^64 within
  reason), two's-complement-mask to `u64`:
  `(seed.rem_euclid(1 << 64)) as u64`; beyond-i128 seeds are a
  clean rejected-input error.
- `vel` lane values: `i64`; `step` index: `i64` (fits easily).

---

## Appendix · Quick quirk reference

Ten facts most likely to surprise mid-build (all specified above):
sixteenth-note defaults for `clock` *and* swing sub; swing only on
exact odd multiples (off-grid events are never swung); lane `div`
default binds at lane-parse time; gate tokens join with no separator
(spaces legal); accent = ×1.15 round-half-away cap 127; `flt ∈ [0,1]`
inclusive; rounded `performed_s` drives sort *and* placement; s16
range is ±32767 symmetric; normalization threshold is strictly
`> 0.98` targeting 0.98; the JSONL can emit `-0.0` (only from a
`-0.0` input, e.g. `time [-0ms]`).

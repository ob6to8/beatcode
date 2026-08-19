# SPEC-GAPS — decisions where SPEC.md under-determines behavior

Per [PLAN.md](PLAN.md): when the spec under-determines something, make
the call, record it here (section cite + choice + why), and continue.

## 1 · §12.5 outside the exactness domain: keep the value, don't assert

§12.5 says to "assert or error" when |x| > 2^53/10^n, but
`goldens/semantics-probes.txt` §E8 records the oracle *keeping the
value* there, and §5.9 wants clean errors everywhere. Chose: `round_dec`
returns the input unchanged outside the domain (matches §E8 exactly);
`format_dec` falls back to Rust's shortest-round-trip `Display` there.
Why: conforms to the probe, avoids an unclean panic. Reachable from
grammar-legal extremes (a huge verbatim `gain=` literal, or a
`performed_s` past ~9.0e9 s via an enormous `time` lane), so the
fallback keeps §7's "at least one fractional digit" by appending
`.0` to integral Display output; non-finite values cannot reach
serialization at all (#7).

## 2 · `swing <amt> <tok>`: a present-but-malformed sub errors

§5.2 gives `swing <amt>[%] [<notefrac>]` and says extra tokens after
the *consumed* ones are ignored. When a third token is present, this
implementation consumes it as the sub and raises "bad note fraction"
if it doesn't parse (`swing 58 bpm` is an error; `swing 58% 1/16 bpm`
is fine). Why: the sub position is a declared consumable of the
grammar, unlike the trailing garbage the examples rely on; silently
ignoring a malformed sub would mask typos in the one numeric knob
swing has. No golden exercises this shape.

## 3 · `voice` with no name

§5.3 says the name is "the next token" but not what happens when the
line is just `voice`. Chose: clean parse error `voice needs a name`
citing the line. Why: §5.9's model (clean line-cited errors); no
golden exercises it.

## 4 · Missing argument for fixed-arity directives

`tempo` / `bars` / `seed` / `hum` / `prob` / `clock` with no argument
is not specified (§5.2/§5.4 give grammars only for the well-formed
shapes; the reference's behavior is unrecorded except for bare `swing`,
which crashed — §E6). Chose: clean line-cited error
`'<kw>' needs an argument`, mirroring the §E6 posture for bare
`swing` (which this implementation also rejects cleanly, per spec).

## 5 · `loop` renders once on startup

§10 defines `loop` as "poll the file's mtime every 200 ms; on change
re-render + play" — whether the freshly started loop performs an
initial render is unspecified. Chose: the first poll treats the
existing file as a change (render + play immediately, or `!! …` if the
score is currently broken), then watch. Why: a jam loop that stays
silent until the first save is surprising, and announcing a pre-existing
typo immediately beats waiting for the user to save again.

## 6 · Receipt seconds format trims trailing zeros

§9.8/§6.10 fix WAV seconds at 2 decimals via the one §12.5 routine,
whose formatting half trims trailing zeros keeping ≥ 1 fractional
digit — so exactly 2.60 s prints `2.6s`, not `2.60s`. Chose: use the
shared routine unmodified (the spec's "one routine serves" §12.5 note).
Informative surface either way.

## 7 · Non-finite values are rejected, mirroring the oracle's raises

§5.8's num! grammar admits literals whose f64 value overflows
(`1e999`), and finite extremes can overflow mid-pipeline
(`time [1e308]` at a huge step length) — Rust would carry `inf`/`NaN`
into `performed_s`, contradicting §2.3's "performed_s is finite" and
emitting invalid JSONL. The oracle cannot reach that state at all: its
floats have no infinities (`Float.parse` errors on overflow; float
arithmetic raises badarith, exactly as the transcript shows for
`tempo 0`). Chose: (a) num! rejects tokens whose value is non-finite
("bad number", line-cited); (b) a subnormal tempo whose `spb`
overflows is a clean compile error like the tempo-0 posture (#11);
(c) the STORED event fields (`swing_ms`/`lane_ms`/`hum_ms` after the
×1000 scaling and rounding, and `performed_s`) are checked finite
before serialization — the check runs on exactly what JSONL would
emit, so a finite seconds value whose ×1000 overflows is also caught;
(d) `decfmt` defensively passes non-finite inputs through unchanged
instead of panicking. Accept/reject matches the oracle on every such
input.

## 8 · Enormous `bars` values run unbounded, like the oracle

`bars 400000000` is grammar-legal, passes `bars ≥ 1`, and compiles
~6.4e9 steps per voice — minutes of CPU or an OOM kill, not a clean
error. The oracle behaves identically (its comprehension over the
step range is just as unbounded), §3 calls large magnitudes merely
impractical, and any cap would arbitrarily reject scores the oracle
accepts (e.g. `bars 100000` compiles fine in seconds). Chose: no cap;
genuine i64/rational overflow still surfaces the clean "rational
overflow" error.

## 9 · Renders past the WAV container's u32 sizes are a clean error

§9.7's header carries `data_size` and `36 + data_size` as u32 fields,
capping a render at (2^32 − 1 − 36)/4 frames ≈ 6.76 hours. A
grammar-legal score can place an event far beyond that
(`time [1e18ms]` → performed_s ≈ 1e15 s — Class A `events` output is
fine), where naive placement arithmetic would wrap/panic and even a
"successful" file would lie about its size. Chose: `render` checks
each event's placement against the container bound and raises a clean
error ("render too long for the WAV container"); `events` is
unaffected. Why: §5.9's clean-error rule; the bound is the format's
own physical limit, not an arbitrary cap.

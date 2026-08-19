# SPEC-GAPS — decisions where SPEC.md under-determines behavior

Per [PLAN.md](PLAN.md): when the spec under-determines something, make
the call, record it here (section cite + choice + why), and continue.

## 1 · §12.5 outside the exactness domain: keep the value, don't assert

§12.5 says to "assert or error" when |x| > 2^53/10^n, but
`goldens/semantics-probes.txt` §E8 records the oracle *keeping the
value* there, and §5.9 wants clean errors everywhere. Chose: `round_dec`
returns the input unchanged outside the domain (matches §E8 exactly);
`format_dec` falls back to Rust's shortest-round-trip `Display` there.
Why: conforms to the probe, avoids an unclean panic reachable only
through adversarial inputs (e.g. astronomically large verbatim
`gain=`/`pan=` literals — every ms/performed field is value-rounded
first and is orders of magnitude inside the domain).

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

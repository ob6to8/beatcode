# beatcode

An offline, deterministic music compiler and renderer. Plain-text
scores of voices with independently-cycling lanes compile through a
fixed-order timing pipeline into a dual-time event list, then render
to WAV with a built-in synthesized kit — **same score + same seed ⇒
byte-identical WAV, on any machine**; the sha256 is printed to prove
it.

```
score.bc ──parse──▶ Score ──compile──▶ [Event] ──┬─▶ JSONL on stdout   (events)
                                                 └─▶ WAV + sha256      (render)
                                                        └─▶ play / loop
```

A taste of the score format (full grammar in [SPEC.md](SPEC.md) §5):

```
tempo 88
bars 2
seed 41
swing 58% 1/16              global swing, MPC convention (50 = straight)

voice kick sample=kick
  gate x..x......x..x..     x hit · X accent · . rest — cycles at its own length
  time [-4ms 0 -7ms 0] div=1/4
  hum 1

voice bass synth=pluck
  gate x..x.x..x..x.x..
  pitch [e1 g1 a1 e1 d2 a1] div=1/8
  swing off
```

Lanes are the load-bearing idea: gate, vel, pitch, and time each
cycle at **their own length and division**, so mismatched lane
lengths produce evolving patterns and polymeter for free
(`examples/poly.bc` is nothing but that trick).

## Status

Specification seed. This repo currently contains the complete
behavioral spec, its golden conformance vectors, and the example
scores; the implementation is built from them per
[PLAN.md](PLAN.md).

| | |
|---|---|
| [SPEC.md](SPEC.md) | the exact observable behavior, down to rounding rules and byte formats |
| [PLAN.md](PLAN.md) | phased build plan with acceptance gates |
| [goldens/](goldens/README.md) | frozen machine-checkable vectors backing every normative claim |
| [examples/](examples/) | four scores: straight house, a Dilla-feel groove, a polymeter study, and an edge-case exerciser |

## Commands (once built)

```
bc events <score.bc>          compiled events as JSONL (pipe to jq)
bc render <score.bc> [out]    render WAV, print sha256 + event count
bc play   <score.bc>          render + play once
bc loop   <score.bc>          re-render + play on every save (the jam loop)
bc demo                       render all examples/
```

## License

[Apache 2.0](LICENSE).

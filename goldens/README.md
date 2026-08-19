# goldens — frozen conformance vectors

Machine-checkable ground truth for [SPEC.md](../SPEC.md). **Frozen:
never edit or regenerate these files**; the implementation conforms
to them, not the other way around.

Floats in the vector files are printed shortest-round-trip: parse to
f64 and compare **bit patterns**. The sign of zero is significant
wherever printed.

| File | Validates | Notes |
|---|---|---|
| `events/four.events.jsonl` | full event list for `examples/four.bc` (24 events) | byte-exact target |
| `events/dilla.events.jsonl` | `examples/dilla.bc` (44) | byte-exact target |
| `events/poly.events.jsonl` | `examples/poly.bc` (86) | byte-exact target |
| `events/edge.events.jsonl` | `examples/edge.bc` (58) | byte-exact target; exercises the SPEC §6.12 worked examples |
| `prng-vectors.jsonl` | 87 vectors: fnv-1a, key chains (with intermediate keys), flt outputs, noise streams | SPEC §4 |
| `float-semantics.txt` | rounding / formatting / parsing probes | two lines expected-to-diverge: the `ftb(5.0e-7)` formatter boundary (SPEC §6.10) and the final `pow` probe (transcendental, platform-scoped) |
| `parser-behaviors.txt` | 42 accept/reject cases with cited line numbers and event summaries | error *texts* are historical, informative only; accept/reject + line numbers are the contract; two cases expected-to-diverge by decided posture (SPEC §5.10): `bars 0` and negative `vel` — the implementation rejects both |
| `semantics-probes.txt` | edge-rule transcripts: sign of zero (E1), pitch serialization on any voice (E2), formatter boundary (E3), CRLF (E4), suffix trimming (E5), error shapes (E6), rounding-domain edge (E8) | probe ids are cited from SPEC.md |

The implementation adds its own render-hash goldens
(`renders-v0.1.txt`) in Phase 3 of [PLAN.md](../PLAN.md).

# Build plan — bc v0.1

Implement `bc` per [SPEC.md](SPEC.md). **Everything needed is in this
repository** — the spec, the golden vectors, the example scores. No
external references are required or expected.

## Method

Spec-driven: SPEC.md is the contract, `goldens/` is the ground truth.
When the spec under-determines something you need: make the call,
record it in a `SPEC-GAPS.md` at the repo root (section cite + what
you chose + why), and continue — do not stall. When a golden and the
spec prose seem to disagree, the golden wins unless it is one of the
four expected-to-diverge cases listed in SPEC §11.2.

Work on branch `build/v0.1`; open a PR to `main` when acceptance is
green. Commit in reviewable increments (roughly one module or one
phase gate per commit).

## Phases — gate on the named checks before moving on

**Phase 0 — scaffold.** Cargo binary crate `bc` (edition 2024, zero
dependencies, `license = "Apache-2.0"` matching the repo's LICENSE),
`rust-toolchain.toml` pinning current stable, CI workflow (below),
`.gitignore`. Golden checks live in `tests/` as integration tests
reading `goldens/` and `examples/`.

**Phase 1 — the event compiler (Class A).** `rational` → `prng` →
`decfmt` (SPEC §12.5 exactly, including the sign-of-zero branch) →
`score` parser → `events` pipeline → `jsonl`. Tests, all against the
vendored goldens: 87/87 PRNG vectors (compare keys as u64, floats by
bit pattern); `bc events` byte-equals all four
`goldens/events/*.events.jsonl`; the 42 `parser-behaviors.txt` cases
(accept/reject + cited line numbers; 40 match + the two
expected-to-diverge cases asserted as *rejections*); the
`float-semantics.txt` probes minus its two expected-to-diverge lines.
*Gate: SPEC §11.3 items 2–4.*

**Phase 2 — the sound half.** `sha256` (FIPS 180-4, standard
vectors), `wav` (§9.7 — header-bytes test), `synth` (§8 shapes
realized transcendental-free per §8.4: pinned odd-polynomial sine
with in-source coefficient docs, precomputed envelope constants as
literals with derivations, `x·sqrt(x)`, integer-exponent note
frequency), `render` (§9 exactly: placement from the *rounded*
`performed_s`, event-then-frame summation order, `last + 22050` tail,
empty ⇒ 44100 frames, `peak > 0.98` ⇒ scale `0.98/peak`, symmetric
±32767 ties-away s16, LE interleave). *Gate: §11.3 item 6, plus a
double-render byte-equality test.*

**Phase 3 — CLI and receipts.** §10 exactly: `events`, `render`
(receipt line with two-space separators and the `(peak-normalized)`
flag; seconds = frames/44100 rounded 2 decimals by the §6.10 rule),
`play` (12-hex receipt + player fallback chain), `loop` (200 ms
mtime poll; errors print `!! <msg> (fix and save again)` and the loop
keeps watching), `demo`. Clean line-cited errors, non-zero exits.
Port the six §11.1 properties as tests. Render the four example
scores and commit their sha256s as `goldens/renders-v0.1.txt` with
the platform recorded. *Gate: §11.3 items 5 and 8, and item 7's
double-render leg.*

**Phase 4 — CI proves the flagship.** Matrix `{ubuntu-latest,
macos-latest}`: fmt --check, clippy (deny warnings), `cargo test`,
then render the four scores and diff their sha256s against
`goldens/renders-v0.1.txt` — both OSes matching the committed hashes
*is* the cross-machine determinism claim, re-proven on every push.
Include the banned-token check from SPEC §8.4. *Gate: §11.3 items 1
and 7 complete → tag `v0.1`, open the PR.*

## Definition of done

SPEC §11.3, items 1–8, all green, with `SPEC-GAPS.md` current (state
explicitly if it is empty). The PR description reports: the
acceptance checklist, the committed render hashes, and the SPEC-GAPS
list.

#!/usr/bin/env bash
# Re-render the four example scores and compare their sha256s against the
# committed goldens/renders-v0.1.txt (PLAN.md Phase 3/4; SPEC §11.3 item 7).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --quiet

fail=0
while read -r name want; do
  case "$name" in ''|'#'*) continue ;; esac
  score="examples/${name%.wav}.bc"
  out="renders/ci-check/$name"
  line=$(./target/release/bc render "$score" "$out")
  got=$(printf '%s\n' "$line" | sed -E 's/.*sha256=([0-9a-f]{64}).*/\1/')
  if [ "$got" != "$want" ]; then
    echo "MISMATCH $name: got $got want $want"
    fail=1
  else
    echo "ok $name  $got"
  fi
done < goldens/renders-v0.1.txt

exit $fail

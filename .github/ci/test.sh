#!/usr/bin/env bash
# The kit's tests: the workspace, the SDK's physics features, the font library,
# fresh games from both templates and from every genre starter kit (each must
# pass its own tests, then build and validate), and `mb serve --watch`.
#
# One script, run in two places so they can't drift:
#   - the kit's own release workflow (.github/workflows/release.yml, `test` job)
#     on Linux, macOS and Windows (Git Bash), before it builds the mb binaries;
#   - the monorepo's PR CI (`Kit tests` in terracatta/maimbrain's ci.yml), on a
#     kit assembled with tools/kit/build.sh exactly as the release assembles it.
#
# Usage, from anywhere: bash .github/ci/test.sh
# Needs Rust with the wasm32-unknown-unknown target, and curl.
#   RUNNER_TEMP    where the scratch games go (set in CI; else a fresh temp dir)
#   KIT_TEST_PORT  the port for mb serve --watch (default 8765)
set -euo pipefail
cd "$(dirname "$0")/../.."
tmp=${RUNNER_TEMP:-$(mktemp -d)}
port=${KIT_TEST_PORT:-8765}
# Windows Git Bash: D:/a/..., which Cargo understands (pwd alone gives /d/a/...).
kit="$(pwd -W 2>/dev/null || pwd)"

step() { { set +x; } 2>/dev/null; printf '\n=== %s\n' "$*"; set -x; }

serve_pid=
cleanup() { if [ -n "$serve_pid" ]; then kill "$serve_pid" 2>/dev/null || true; fi; }
trap cleanup EXIT

step "cargo test --workspace"
cargo test --workspace

step "The SDK's physics features (Rapier; off by default)"
cargo test -p maimbrain --features physics2d,physics3d

# Fonts themselves download on demand; mb new below bakes some.
step "The font library parses and lists"
cargo run -q -p mb-cli -- fonts --category pixel

# A fresh game from each template must pass its own tests (rules, and the bot's
# difficulty report) and build and validate (needs the wasm target), created
# outside this workspace and pointed at this checkout's SDK.
step "New games from both templates test, build and validate"
cargo run -q -p mb-cli -- new "$tmp/ci2d" --id com.example.ci2d --creator @ci
cargo run -q -p mb-cli -- new "$tmp/ci3d" --3d --id com.example.ci3d --creator @ci
# The 3D game also turns on physics3d, so Rapier is built for wasm32 here.
sed -i.bak 's|maimbrain = { git = "https://github.com/terracatta/maimbrain-kit" }|maimbrain = { git = "https://github.com/terracatta/maimbrain-kit", features = ["physics3d"] }|' "$tmp/ci3d/Cargo.toml"
grep -q physics3d "$tmp/ci3d/Cargo.toml"
for g in ci2d ci3d; do
  printf '\n[patch."https://github.com/terracatta/maimbrain-kit"]\nmaimbrain = { path = "%s/sdk/maimbrain" }\n' "$kit" >> "$tmp/$g/Cargo.toml"
  cargo test -q --manifest-path "$tmp/$g/Cargo.toml"
  cargo run -q -p mb-cli -- build "$tmp/$g"
done

# Every genre starter kit (`mb new --kit`) must scaffold into a game that passes
# its own tests (rules and bot sanity checks) and builds and validates.
step "New games from every kit test, build and validate"
cargo build -q -p mb-cli
target/debug/mb kits
# One target dir for all of them, so the SDK (and Rapier) compile once.
shared="$tmp/kit-target"
for k in runner stacker shooter match3 racer tower-defense trivia; do
  g="kit_$(echo "$k" | tr - _)"
  target/debug/mb new --kit "$k" "$tmp/$g" --id "com.example.$(echo "$k" | tr -d -)" --creator @ci
  printf '\n[patch."https://github.com/terracatta/maimbrain-kit"]\nmaimbrain = { path = "%s/sdk/maimbrain" }\n' "$kit" >> "$tmp/$g/Cargo.toml"
  CARGO_TARGET_DIR="$shared" cargo test -q --manifest-path "$tmp/$g/Cargo.toml"
  CARGO_TARGET_DIR="$shared" target/debug/mb build "$tmp/$g"
done

# mb serve --watch builds, serves, and rebuilds after an edit (no browser: the
# status endpoints say what was built).
step "mb serve --watch rebuilds on save"
target/debug/mb serve --watch "$tmp/ci2d" --port "$port" > "$tmp/serve.log" 2>&1 &
serve_pid=$!
trap 'cleanup; cat "$tmp/serve.log"' EXIT
for _ in $(seq 1 60); do curl -sf "http://127.0.0.1:$port/__mb/status" > /dev/null && break; sleep 1; done
curl -sf "http://127.0.0.1:$port/__mb/wait?timeout=300" | tee "$tmp/first.json"
grep -q '"state":"ok"' "$tmp/first.json"
curl -sf "http://127.0.0.1:$port/game/game.wasm" -o /dev/null
echo "// edited" >> "$tmp/ci2d/src/lib.rs"
curl -sf "http://127.0.0.1:$port/__mb/wait?timeout=300" | tee "$tmp/second.json"
grep -q '"build":2' "$tmp/second.json"
grep -q '"state":"ok"' "$tmp/second.json"

{ set +x; } 2>/dev/null
echo
echo "=== The kit's tests passed."

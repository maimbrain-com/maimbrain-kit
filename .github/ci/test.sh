#!/usr/bin/env bash
# The kit's tests, in parts (all of them, in this order, when none are named):
#   workspace  cargo test --workspace, and the SDK's physics features
#   templates  a fresh game from each template passes its own tests, then builds and validates
#   kits       the same for a game from every genre starter kit (`mb new --kit`)
#   serve      `mb serve --watch` builds, serves, and rebuilds after an edit
# and three that do less, for runs that split the work:
#   unit       `workspace` less five long audio and mesh processing tests in mb (the
#              same arithmetic on every OS, and the monorepo's CI runs them)
#   mb         `unit` for mb and mb-format alone
#   smoke      the font library lists, and a game from a template and one from a kit
#              are made and build (no game tests: game code is the same everywhere)
#
# One script, run in two places so they can't drift:
#   - the kit's own release workflow (.github/workflows/release.yml, `test` job),
#     on pull requests to the kit, on Linux, macOS and Windows (Git Bash);
#   - the monorepo's PR CI (`Kit tests` in terracatta/maimbrain's ci.yml), on a
#     kit assembled with tools/kit/build.sh exactly as the release assembles it.
#     There Linux runs `unit` and `templates kits serve` side by side, and macOS
#     and Windows run `mb smoke serve`: what can differ between platforms.
#
# Usage, from anywhere: bash .github/ci/test.sh [part...]
# Needs Rust with the wasm32-unknown-unknown target, and curl.
#   RUNNER_TEMP    where the scratch games go (set in CI; else a fresh temp dir)
#   KIT_TEST_PORT  the port for mb serve --watch (default 8765)
set -euo pipefail
cd "$(dirname "$0")/../.."
tmp=${RUNNER_TEMP:-$(mktemp -d)}
port=${KIT_TEST_PORT:-8765}
# Windows Git Bash: D:/a/..., which Cargo understands (pwd alone gives /d/a/...).
kit="$(pwd -W 2>/dev/null || pwd)"
# One target dir for every scratch game, so the SDK (and Rapier) compile once.
games_target="$tmp/games-target"
parts=("$@")
[ ${#parts[@]} -gt 0 ] || parts=(workspace templates kits serve)
for p in "${parts[@]}"; do
  case "$p" in workspace | unit | mb | templates | kits | smoke | serve) ;; *) echo "unknown part: $p" >&2; exit 2 ;; esac
done

# Without the long processing tests (a run that leaves out `workspace`), the
# optimized dependencies in the kit's Cargo.toml cost far more to compile than
# they save: build unoptimized and without debug info instead. On four cores,
# `mb` compiles in 38 s instead of 188 s, and its tests take 42 s instead of 15 s.
build=()
case " ${parts[*]} " in
  *" workspace "*) ;;
  *) build=(--config 'profile.dev.package."*".opt-level=0' --config 'profile.dev.package.mb-cli.opt-level=0' --config 'profile.dev.debug=0') ;;
esac
# "${build[@]}" alone is an unbound variable to bash 3.2 (macOS) when empty.
cargo_() { local cmd=$1; shift; cargo "$cmd" ${build[@]+"${build[@]}"} "$@"; }

step() { { set +x; } 2>/dev/null; printf '\n=== %s\n' "$*"; set -x; }

serve_pid=
cleanup() { if [ -n "$serve_pid" ]; then kill "$serve_pid" 2>/dev/null || true; fi; }
trap cleanup EXIT

mb=
build_mb() {
  if [ -z "$mb" ]; then
    cargo_ build -q -p mb-cli
    mb=target/debug/mb
  fi
}

# A new game outside this workspace, pointed at this checkout's SDK: from the
# template (`2d`, `3d`) or a genre starter kit (anything else). Made once.
new_game() {
  local g=$1 from=$2
  [ -d "$tmp/$g" ] && return
  build_mb
  case "$from" in
    2d) "$mb" new "$tmp/$g" --id "com.example.$g" --creator @ci ;;
    3d)
      "$mb" new "$tmp/$g" --3d --id "com.example.$g" --creator @ci
      # The 3D game also turns on physics3d, so Rapier is built for wasm32 here.
      sed -i.bak 's|maimbrain = { git = "https://github.com/terracatta/maimbrain-kit" }|maimbrain = { git = "https://github.com/terracatta/maimbrain-kit", features = ["physics3d"] }|' "$tmp/$g/Cargo.toml"
      grep -q physics3d "$tmp/$g/Cargo.toml"
      ;;
    *) "$mb" new --kit "$from" "$tmp/$g" --id "com.example.$(echo "$from" | tr -d -)" --creator @ci ;;
  esac
  printf '\n[patch."https://github.com/terracatta/maimbrain-kit"]\nmaimbrain = { path = "%s/sdk/maimbrain" }\n' "$kit" >> "$tmp/$g/Cargo.toml"
}

# A game passes its own tests (rules, and the bot's difficulty report or sanity
# checks), then builds and validates (needs the wasm target).
test_game() {
  CARGO_TARGET_DIR="$games_target" cargo_ test -q --manifest-path "$tmp/$1/Cargo.toml"
  build_game "$1"
}
build_game() { CARGO_TARGET_DIR="$games_target" "$mb" build "$tmp/$1"; }

kits=(runner stacker shooter match3 racer tower-defense trivia)
# Their time goes to synthesizing, resampling and analyzing audio and simplifying
# meshes (crates/mb-cli/tests/generate.rs), which no platform does differently.
long=(music_in_sections_and_layers_for_a_take_you_have music_and_sfx models_regen_variants_and_check
  a_model_made_earlier_is_rigged_and_gets_clips_without_rigging_again reprocessing_a_model_keeps_its_recipe_and_its_place)
skip_long=(--exact)
for t in "${long[@]}"; do skip_long+=(--skip "$t"); done

for p in "${parts[@]}"; do
  case "$p" in
    workspace)
      step "cargo test --workspace"
      cargo test --workspace
      step "The SDK's physics features (Rapier; off by default)"
      cargo test -p maimbrain --features physics2d,physics3d
      ;;
    unit)
      step "cargo test --workspace, less the long audio and mesh processing tests"
      cargo_ test --workspace -- "${skip_long[@]}"
      step "The SDK's physics features (Rapier; off by default)"
      cargo_ test -p maimbrain --features physics2d,physics3d
      ;;
    mb)
      step "mb's and mb-format's own tests, less the long audio and mesh processing"
      cargo_ test -p mb-cli -p mb-format -- "${skip_long[@]}"
      ;;
    templates)
      # Fonts themselves download on demand; mb new bakes some.
      step "The font library parses and lists"
      build_mb
      "$mb" fonts --category pixel
      step "New games from both templates test, build and validate"
      new_game ci2d 2d
      new_game ci3d 3d
      test_game ci2d
      test_game ci3d
      ;;
    kits)
      step "New games from every kit test, build and validate"
      build_mb
      "$mb" kits
      for k in "${kits[@]}"; do
        g="kit_$(echo "$k" | tr - _)"
        new_game "$g" "$k"
        test_game "$g"
      done
      ;;
    smoke)
      step "The font library lists; a game from a template and one from a kit build and validate"
      build_mb
      "$mb" fonts --category pixel
      "$mb" kits
      new_game ci2d 2d
      new_game kit_runner runner
      build_game ci2d
      build_game kit_runner
      ;;
    serve)
      # No browser: the status endpoints say what was built.
      step "mb serve --watch rebuilds on save"
      new_game ci2d 2d
      CARGO_TARGET_DIR="$games_target" "$mb" serve --watch "$tmp/ci2d" --port "$port" > "$tmp/serve.log" 2>&1 &
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
      cleanup
      wait "$serve_pid" 2>/dev/null || true
      serve_pid=
      ;;
  esac
done

{ set +x; } 2>/dev/null
echo
echo "=== The kit's tests passed: ${parts[*]}"

#!/usr/bin/env bash
# Regenerates Nova Pip's art and music with `mb art` and `mb music`
# (docs/GENERATE.md), and synthesizes its effects (tools/synth_sfx.py).
# Every image follows art/style.toml; the prompts below say what each asset
# IS. To restyle the kit: edit art/style.toml (style, palette, [music]
# words), rewrite the prompts here for your theme (and the effect recipes in
# tools/synth_sfx.py), then run the steps in order and look at every preview
# in art/preview/.
#
#   tools/regen.sh hero        Pip, 3 candidates (pick one, then `ref`)
#   tools/regen.sh ref         Pip's raw image becomes the style reference (art/ref/pip.jpg)
#   tools/regen.sh sprites     every other sprite into art/sprites/, then `atlas`
#   tools/regen.sh jelly boss  just these sprites (any names below), then `atlas`
#   tools/regen.sh atlas       pack art/sprites/*.png into assets/sprites.png + src/sprites.rs, quantize
#   tools/regen.sh backdrop | icon | music
#   tools/regen.sh sfx         every effect, synthesized (tools/synth_sfx.py; free, offline)
#   tools/regen.sh all         hero (first candidate), ref, sprites, backdrop, icon, music, sfx: about $1.15
#
# Sprite sources live in art/sprites/ (their .gen.json sidecars are kept in
# git, the PNGs aren't: they're packed into the atlas). `atlas` first restores
# any missing source PNG from the current atlas, so regenerating one sprite
# keeps the others. After regenerating a sprite with a face window, run
# tools/measure_faces.sh and update the offsets at the top of src/draw.rs.
#
# Needs: mb (or MB="cargo run -q -p mb-cli --"), python3 with Pillow for the
# atlas quantize step, ImageMagick (`magick`) to restore sprites from the atlas,
# numpy for the effects.
set -euo pipefail

G="$(cd "$(dirname "$0")/.." && pwd)"
MB=${MB:-mb}
mb() { $MB "$@"; }

# What every asset is. Keep faces out of the sprites ("plain dark empty
# oval"): the code draws eyes there that follow the player.
NOFACE="no eyes and no face: the middle of the body is a plain dark empty oval where a face will be added later"

P_PIP="a sleek little spaceship pod pointing straight up, seen from above: a rounded capsule body like a teardrop with a pointed nose at the top, two short swept-back wings, a big dark empty visor window across the upper body (an empty glass dome, nothing inside it), cyan neon outline with magenta accents on the wing tips, an engine nozzle at the bottom"

sprite() { # name size prompt [extra args]
  local n=$1 size=$2 prompt=$3; shift 3
  mb art sprite "$G" "$n" "$prompt" --size "$size" --out "art/sprites/$n.png" "$@"
}

one() {
  case $1 in
    pip) sprite pip 128x128 "$P_PIP" ;;
    jelly) sprite jelly 96x96 "a small glowing jellyfish creature: a round dome-shaped bell with four short wavy tentacles hanging below, hot magenta-pink neon outline, $NOFACE" ;;
    swooper) mb art frames "$G" swooper "a small bat-like swooping creature with angular swept wings, orange neon outline, a round body in the middle, flapping its wings (wings up, level, down, level), $NOFACE" --frames 4 --size 112x112 --out art/sprites/swooper.png ;;
    darter) sprite darter 96x96 "a sleek arrowhead-shaped dart creature pointing to the RIGHT, a sharp kite-shaped body with two small fins at the back, electric violet neon outline, $NOFACE" ;;
    bulb) sprite bulb 128x128 "a big round lantern-like bulb creature, a fat sphere with a crown of six short rounded spikes around its upper half, golden yellow neon outline, $NOFACE" ;;
    spinner) sprite spinner 128x128 "a six-pointed star creature like a throwing star, perfectly rotationally symmetric, coral red neon outline with a white-hot inner line, a plain dark empty disc in the center, no eyes, no face" ;;
    boss) sprite boss 320x256 "a huge menacing queen creature for a boss fight, seen straight on: a wide dome-shaped jellyfish head, much wider than tall, with a tall spiky three-pointed crown on top set with glowing gems, hot magenta and electric violet neon outlines with a gold crown, a ruffled frilled rim along the bottom edge of the dome, and in the middle of the dome a large plain dark empty face plate (no eyes, no mouth, no face), no tentacles, no arms" ;;
    hand) sprite hand 64x64 "a single glowing round claw orb, a sphere with three short curved claws gripping it from below, magenta neon outline, dark violet glass inside" ;;
    gem) sprite gem 48x48 "a small faceted diamond crystal, tall rhombus shape, golden yellow neon outline with white-hot facet lines, dark amber glass inside" ;;
    heart) sprite heart 64x64 "a plump heart shape, hot pink-red neon tube outline with a white-hot core line, dark red glass inside, a small white shine on the upper left" ;;
    pw_spread) sprite pw_spread 64x64 "a round power-up orb token: a glowing circle with three bold arrows fanning upward out of its center, cyan neon" ;;
    pw_shield) sprite pw_shield 64x64 "a round power-up orb token: a glowing circle with a bold shield emblem in its center, royal blue and white neon" ;;
    pw_magnet) sprite pw_magnet 64x64 "a round power-up orb token: a glowing circle with a bold horseshoe magnet emblem in its center, hot magenta and white neon" ;;
    *) echo "unknown asset: $1" >&2; exit 2 ;;
  esac
}

SPRITES="jelly swooper darter bulb spinner boss hand gem heart pw_spread pw_shield pw_magnet"

hero() {
  if grep -q '^references = \[ *"' "$G/art/style.toml"; then
    echo "note: art/style.toml has references; for a new look set references = [] before making the hero" >&2
  fi
  sprite pip 128x128 "$P_PIP" --variants "${1:-3}"
  echo "then: mb art pick $G pip <n> (it lands in art/sprites/pip.png), tools/regen.sh ref, and tools/regen.sh atlas"
}

ref() {
  # The hero's raw 1K image (not the 128 px sprite) carries the style best.
  local raw
  raw=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['raw'])" "$G/art/sprites/pip.png.gen.json")
  mkdir -p "$G/art/ref"
  magick "$G/$raw" -resize 384x384 -quality 85 "$G/art/ref/pip.jpg"
  echo "wrote art/ref/pip.jpg: set references = [\"art/ref/pip.jpg\"] in art/style.toml"
}

atlas() {
  # Restore source PNGs missing from art/sprites/ by cropping them out of the current atlas.
  if [ -f "$G/assets/sprites.png" ] && [ -f "$G/src/sprites.rs" ]; then
    for n in pip $SPRITES; do
      [ -f "$G/art/sprites/$n.png" ] && continue
      local up rect
      up=$(echo "$n" | tr '[:lower:]' '[:upper:]')
      rect=$(sed -n "s/^pub const $up: \[f32; 4\] = \[\(.*\)\];/\1/p" "$G/src/sprites.rs" | tr -d ' ' | sed 's/\.0//g')
      [ -n "$rect" ] || continue
      IFS=, read -r x y w h <<< "$rect"
      magick "$G/assets/sprites.png" -crop "${w}x${h}+${x}+${y}" +repage "PNG32:$G/art/sprites/$n.png"
      echo "restored art/sprites/$n.png from the atlas"
    done
  fi
  mb art atlas "$G" sprites "$G"/art/sprites/*.png
  python3 "$G/tools/quantize.py" "$G/assets/sprites.png"
}

backdrop() {
  mb art background "$G" backdrop "a retro synthwave night: a deep indigo sky fading to violet near the horizon, a faint scattering of tiny stars and a soft purple nebula high up; the horizon is a straight level line exactly halfway down the image; on the horizon a big setting neon sun, half sunk below the line, cut by horizontal dark stripes, glowing magenta at the bottom to warm gold at the top, softly dimmed; low wireframe mountain silhouettes in magenta neon lines either side of the sun along the horizon; below the horizon the ground is an empty, plain, very dark indigo floor with no lines, no grid and no detail" --size 540x960 --max-kb 80
  echo "check where the horizon landed and set HORIZON in src/draw.rs (texel row / 960 * 640)"
}

icon() {
  mb art icon "$G" "the little neon pod spaceship from the reference, big and centered, flying straight up toward the top, its dark visor showing two big round friendly white eyes, a cyan thruster flame below it; behind it a striped neon synthwave sun on a glowing grid horizon, two small magenta jellyfish enemies and hot pink glowing bullets around it" --max-kb 60
}

music() {
  # The main loop, split into a low and a high layer (sound.rs opens the high
  # layer with the action), and the boss loop. 24 kHz q1 keeps both small.
  mb music "$G" music "driving synthwave chase loop: a pulsing arpeggiated analog bass line in sixteenth notes, big gated-reverb snare on 2 and 4, four-on-the-floor kick, shimmering arpeggiated lead synth, bright detuned saw pads, heroic and urgent, high energy from the first beat, minor key" \
    --bpm 120 --bars 8 --layers --split-hz 900 --rate 24000 --quality 1
  mb music "$G" music_boss "intense synthwave boss battle loop: relentless sixteenth-note distorted analog bass, pounding four-on-the-floor kick with big gated snares and tom fills, a screaming detuned saw lead playing a dark minor riff, siren-like rising synth stabs, menacing and urgent, full intensity from the first beat" \
    --bpm 132 --bars 8 --rate 24000 --quality 1
}

sfx() {
  # The effects are synthesized, not generated: an analog-synth palette in
  # the music's key (G minor; the boss loop's A), deterministic and free.
  # Every sound's recipe is a function in tools/synth_sfx.py; the mix is
  # MIX in src/sound.rs. After regenerating the music, measure its key and
  # update ROOT in synth_sfx.py and KEY_BOSS/PENTA in src/sound.rs.
  python3 "$G/tools/synth_sfx.py" "$@"
  echo "then: python3 <skill>/scripts/check_audio.py --loops hum $G/assets/*.ogg, and tools/mixsim.py on a bot round"
}

[ $# -gt 0 ] || { sed -n '2,25p' "$0"; exit 1; }
named=0
for step in "$@"; do
  case $step in
    hero) hero ;;
    ref) ref ;;
    sprites) for n in $SPRITES; do one "$n"; done; atlas ;;
    atlas) atlas ;;
    backdrop) backdrop ;;
    icon) icon ;;
    music) music ;;
    sfx) sfx ;;
    all) one pip; ref; for n in $SPRITES; do one "$n"; done; atlas; backdrop; icon; music; sfx ;;
    *) one "$step"; named=1 ;;
  esac
done
[ $named = 0 ] || atlas

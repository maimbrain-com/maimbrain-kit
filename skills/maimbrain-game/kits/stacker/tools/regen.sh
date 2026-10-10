#!/bin/sh
# Regenerates every picture and sound in this game with `mb art`, `mb music`
# and `mb sfx`, in the style of art/style.toml, then packs and quantizes them.
# To restyle the game: edit art/style.toml (style, palette, [music], [sfx]),
# edit the prompts below (they name the snacks, the bird and the meadow), and
# run it. Needs `mb` with a Gemini key (images, music) and an ElevenLabs key
# (effects); python3 for the last steps.
#
#   tools/regen.sh all       # everything: ~$1.05 (27 images, 2 tracks, 15 effects)
#   tools/regen.sh snacks    # or one group: snacks, bird, faces, scene, icon, music, sfx
#   tools/regen.sh pack      # only re-pack art/sprites/*.png into assets/ (free)
#
# Look at every art/preview/*.png it prints, regenerate what's off with
# `mb regen art/sprites/<name>.png --new-seed` (or edit its prompt here), then
# `tools/regen.sh pack`. Snack pictures are stretched over their colliders
# (`Kind::size()` in src/sim.rs), so each must fill its box the way the
# collider does: toast a 3:2 rectangle, jelly a square, the baguette a long
# rounded bar, cheese a trapezoid (top 46 % of the bottom), the macaron a
# rounded rectangle, the tomato a circle. Say the shape and proportions in the
# prompt, and "nothing peeks out behind it" (models like to add a dark backing
# layer). If the art's shape differs, change the collider to match the art
# (`Kind::wedge_points`, `Kind::radius`) rather than leave them dishonest.
set -e
cd "$(dirname "$0")/.."
MB=${MB:-mb}
G=.
S=art/sprites
mkdir -p $S art/ref
EDGE="; its silhouette is exactly that shape and nothing peeks out behind it (no darker backing layer or shadow outside its edge); no face"

sprite() { # name WxH prompt [more mb args]
	n=$1 size=$2 p=$3
	shift 3
	$MB art sprite $G "$n" "$p" --size "$size" --margin 0 --out "$S/$n.png" "$@"
}

snacks() {
	# The first snack is the style reference for the rest (art/style.toml `references`).
	sprite toast 132x84 "a thick slice of golden toast shaped as a landscape rectangle one and a half times as wide as it is tall, flat top and bottom, straight sides, slightly rounded corners, an orange-brown crust border around soft pale bread with a few crumbs$EDGE"
	cp $S/toast.png art/ref/toast.png
	# One variant of each colored snack first, then the others redraw it in new colors (--ref).
	sprite jelly_2 88x88 "a cube of wobbly fruit jelly, a perfect square exactly as wide as it is tall, straight sides and gently rounded corners, layered translucent-looking orange papers with a pale highlight strip near the top left$EDGE"
	cp $S/jelly_2.png art/ref/jelly.png
	sprite jelly_0 88x88 "the same jelly cube as the reference picture but in mint green: a perfect square exactly as wide as it is tall, gently rounded corners, a simple pale highlight strip near the top left$EDGE" --ref art/ref/jelly.png
	sprite jelly_1 88x88 "the same jelly cube as the reference picture but in bubblegum pink: a perfect square exactly as wide as it is tall, gently rounded corners, a simple pale highlight strip near the top left$EDGE" --ref art/ref/jelly.png
	sprite baguette 232x44 "a long thin baguette lying flat, five times as wide as it is tall, a straight even loaf with blunt rounded ends, golden crust with four diagonal pale score cuts$EDGE"
	sprite cheese 168x84 "a low, wide wedge of yellow cheese, twice as wide as it is tall, shaped as a symmetric trapezoid: a long flat bottom edge, a short flat top edge less than half as wide as the bottom, straight slanted sides; a few round holes kept well inside the shape$EDGE"
	sprite macaron_1 128x72 "a macaron seen from the side: two pastel mint green shells with a cream filling stripe across the middle, a rounded rectangle almost twice as wide as it is tall with very round ends, flat top and bottom$EDGE"
	cp $S/macaron_1.png art/ref/macaron.png
	sprite macaron_0 128x72 "the same macaron as the reference picture but with pastel pink shells: a rounded rectangle almost twice as wide as it is tall, cream filling stripe across the middle$EDGE" --ref art/ref/macaron.png
	sprite macaron_2 128x72 "the same macaron as the reference picture but with pastel lavender shells: a rounded rectangle almost twice as wide as it is tall, cream filling stripe across the middle$EDGE" --ref art/ref/macaron.png
	sprite tomato_1 92x92 "a round orange tomato forming a perfect circle, its small green star of leaves lying flat on the top inside the circle's outline$EDGE"
	cp $S/tomato_1.png art/ref/tomato.png
	sprite tomato_0 92x92 "the same tomato as the reference picture but bright red all over: a perfect circle, its small green star of leaves lying flat on the top inside the circle's outline$EDGE" --ref art/ref/tomato.png
	sprite tomato_2 92x92 "the same tomato as the reference picture but golden yellow all over: a perfect circle, its small green star of leaves lying flat on the top inside the circle's outline$EDGE" --ref art/ref/tomato.png
}

bird() {
	# Bottom-anchored cells: the feet stay put (they grip the snack), the wings flap.
	$MB art frames $G bird "a flapping cycle of a small round cream-white paper bird facing right: plump round body, a darker cream belly patch, one dot eye, a rosy cheek, a small orange triangle beak, two short orange legs hanging straight down below its belly. Frame 1 wings raised high above its back, frame 2 wings level, frame 3 wings swept down (never lower than its feet), frame 4 wings level again. The body, head and legs stay in exactly the same place in every frame; only the wings move" --frames 4 --size 200x140 --out $S/bird.png
}

faces() {
	# Eight expressions in one row; draw::face_frame maps moods to cells.
	$MB art frames $G faces "eight cute cartoon face expressions made of cut paper pieces, faces only (no head or body behind them): each face is two eyes cut from white paper with dark plum pupils, a dark plum paper mouth and two small pink round cheeks, all the same size and spacing, laid out in a neat grid with lots of space between faces. 1 happy: round eyes, small smile. 2 blinking: eyes closed as short lines, small smile. 3 excited: big sparkly eyes, wide open grin. 4 scared: huge round eyes with tiny pupils, small round open mouth. 5 worried: eyebrows raised in the middle, wavy mouth. 6 very worried: same as 5 plus a light blue sweat drop beside one eye. 7 dozing: eyes closed as downward curves, tiny calm smile. 8 dizzy: eyes are X shapes, small round mouth" --frames 8 --size 96x64 --anchor center --out $S/faces.png
}

scene() {
	sprite hills 720x200 "a wide sunny meadow landscape strip, four times as wide as it is tall: layered rolling green paper hills, two or three round paper trees and bushes on them, little white and yellow flowers; the hills fill the bottom of the strip edge to edge with a flat bottom edge, the top is the hills' wavy skyline (no sky)"
	sprite meadow 720x120 "a wide strip of foreground meadow grass, six times as wide as it is tall: layered bright green paper grass with a wavy tufted top edge, a few small paper flowers (white daisies, yellow buttercups, red poppies), flat bottom edge, filling the strip edge to edge"
	sprite blanket 520x88 "a red and cream gingham picnic blanket lying flat on the grass, seen from a low front angle as a wide flat strip about six times as wide as it is tall, the front edge slightly scalloped"
	sprite plate 340x32 "a white paper picnic plate seen exactly edge-on from the side: a long, very thin flat strip about eleven times as wide as it is tall, flat top, its ends slightly rounded, a thin pale blue stripe along its side"
	sprite pole 32x300 "one tall straight wooden stick standing upright, very thin and about ten times as tall as it is wide, light brown kraft paper with a darker grain stripe, flat ends"
	sprite sun 128x128 "a paper sun: a round golden yellow disc on a slightly larger orange disc, ringed by a dozen hand-cut triangular rays, no face"
	sprite cloud_0 160x96 "a fluffy white paper cloud with three round puffs and a flat bottom, cut from two layers of white and very pale blue paper"
	sprite cloud_1 200x80 "a long low white paper cloud with five small round puffs and a flat bottom, cut from two layers of white and very pale blue paper"
	sprite leaf 48x32 "a single small green paper leaf with a pale vein, pointed oval"
	# The grain multiplied over the sky. --no-style: the style's subject words
	# turn a texture into a picnic pattern. (Gemini refused one wording of this
	# as "recitation"; this one goes through.)
	$MB art tile $G paper "a close-up of a sheet of handmade cream paper, matte, with faint wispy fibers and tiny flecks scattered evenly across it, nothing else" --size 128x128 --no-style --out $S/paper.png
}

icon() {
	$MB art icon $G "a close-up of a tiny wobbly stack of three smiling picnic snacks (a slice of toast, a pink jelly cube, a red tomato on top) on a white paper plate, a little cream paper bird with an orange beak flying just above holding the tomato, bright sunny blue sky and green paper hills behind" --max-kb 40
}

pack() {
	# Everything but the sheets and the tile into one trimmed atlas (src/sprites.rs);
	# the sheets and the tile are copied as they are (their cells must stay put).
	$MB art atlas $G sprites $(ls $S/*.png | grep -v -e /bird.png -e /faces.png -e /paper.png) --trim
	tmp=$(mktemp -d)
	for n in bird faces paper; do
		$MB art atlas $G $n $S/$n.png --padding 0 --rust "$tmp/$n.rs"
	done
	rm -rf "$tmp"
	# mb's atlas is RGBA; indexed is a third of the size and looks the same.
	python3 tools/quantize.py assets/sprites.png assets/bird.png assets/faces.png assets/paper.png
}

music() {
	# The tune, then a 4-bar tension layer locked onto it (same tempo; align_loop
	# stretches and rotates it so they play as stems: sound.rs fades it in).
	$MB music $G music "a jaunty acoustic picnic tune in C major: bouncy plucked ukulele chords, a cheerful whistled melody, light shaker and woodblock, soft kick drum on every beat, walking upright bass" --bpm 112 --bars 8 --quality 2 --rate 32000
	$MB music $G tense "the tension layer of a small acoustic picnic band in C major: nervous staccato pizzicato strings repeating one note in eighth notes, a ticking woodblock on every eighth note, a low tremolo cello, a soft kick drum on every beat, suspenseful but playful, no melody, no whistling, no ukulele" --bpm 112 --bars 4 --quality 2 --rate 32000 --no-style
	python3 tools/align_loop.py assets/music.ogg assets/tense.ogg -q 2
}

sfx() {
	# ElevenLabs allows few requests at once: one at a time.
	while IFS='|' read -r n s p; do
		[ -n "$n" ] && $MB sfx $G "$n" "$p" --seconds "$s"
	done <<'EOF'
drop|0.5|a quick papery flutter and one tiny cheerful bird chirp as a little bird lets go of something
thud|0.5|a soft muffled thud of a slice of bread landing on a paper plate
thud2|0.5|a soft papery pat of a small cake landing on cardboard
land|0.6|a single soft wooden marimba note, warm and round, short decay
neat|0.7|a bright sparkly kalimba twinkle, two quick high notes going up
lost|0.9|a short cartoon slide whistle falling down, ending in a soft plop
ooh|1.4|a small crowd of people going oooh together in suspense
new|0.9|a cheerful quick ukulele strum flourish with a happy whistle
wind|1.8|a gust of wind whooshing across a meadow, rising then fading, leaves rustling
wobble|1.2|a slow wooden creak of a wobbly old table leaning over
collapse|1.6|a comic cartoon collapse: a pile of snacks sliding off a plate, papery scrapes and soft bumps ending in a little clatter
over|1.8|a short comic muted trumpet sting going wah wah wah waaah, descending, sad but funny
best|1.8|a joyful short celebration: a happy whistled flourish and a small crowd cheering and clapping
start|0.9|a bright ukulele strum and a cheerful bird tweet
ui|0.5|a soft paper click, like a finger tapping a card
EOF
}

case "${1:-all}" in
all) snacks && bird && faces && scene && pack && icon && music && sfx ;;
snacks | bird | faces | scene) "$1" && pack ;;
pack | icon | music | sfx) "$1" ;;
*) echo "usage: tools/regen.sh [all|snacks|bird|faces|scene|icon|music|sfx|pack]" >&2 && exit 2 ;;
esac
$MB art check $G

# Mistwood

The showcase for mb3d 2: a fox spirit runs a misty forest trail at night
with a lantern swinging from its tail. Tap to jump the fallen logs; the
coins arcing over a log are the reward for jumping it on time.

- **The verb:** tap (jump). One verb.
- **The 1-second read:** a glowing, cel-shaded fox mid-leap on a moonlit trail between glowing mushrooms. The title card is an attract mode: the fox runs and jumps on its own under "RUN THE MISTWOOD".
- **The first 10 seconds:** the entering tap is also the first jump (wasted, harmless); the first log is 22 m in (about 3 s), with a "TAP TO JUMP THE LOGS" hint until then.
- **Tension and failure:** the trail speeds up from 6.5 to 13 m/s; logs come closer together (never closer than a landing plus 3.5 m). Tripping drops the fox on its haunches, idle, beside the log.
- **Score:** meters run (the Distance board); coins are a side count and +10 pops.

## What it shows (world.rs)

| mb3d 2 feature | Where |
|---|---|
| Skinned, animated glTF | `assets/fox.glb` from `tools/make_fox.py`: Idle (cubic spline + a step blink), Run (speed follows the trail), Jump (once), crossfaded by `gfx3d::Animator` |
| Restyling a loaded model | `root.find("Fox")?.material()?.set_style(..)`: toon bands, ink outline, moonlit rim, no fog |
| Attaching to a bone | the lantern node is parented to `Socket.Tail`; its point light swings with the tail |
| Instancing | 7 leapfrogging chunks, each with instanced trees (40), grass (280), glowing mushrooms (6, instance color tints the glow); coins are one instanced node rebuilt every frame |
| Fog | exponential ground fog (`height_falloff`) fading the sky at the horizon; fireflies and sparks fade in it |
| Outlines and toon | trees, logs, coins, signs, the fox |
| 3D text | world-space "MISTWOOD" and distance boards, screen-sized billboard "+10" pops recolored every frame as they fade |
| Depth of field | focused on the fox each frame (`DepthOfField { focus, range: 1.3, blur: 5 }`) |

Budgets in play (mb.stats()): ~60 draws, ~35 k triangles, ~2 300 instances, 1 skinned draw, ~20 outlines, ~30 shadow casters.

## Not verified on device

Frame time on an iPhone 13 class phone (A15); DOF and outline cost there; touch feel of the jump timing.

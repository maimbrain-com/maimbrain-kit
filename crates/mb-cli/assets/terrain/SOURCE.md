# Ground surfaces

The five built-in ground surfaces a `Terrain` blends (docs/STUDIO-2.md
Decided 70) are made from photographed textures, all CC0 1.0 (public domain):

| Files | Source | Page |
|---|---|---|
| `grass.jpg`, `grass_n.jpg` | **Grass004**, ambientCG.com | https://ambientcg.com/view?id=Grass004 |
| `meadow.jpg`, `meadow_n.jpg` | **Forest Ground 01** (`forrest_ground_01`) by Rob Tuytel, Poly Haven | https://polyhaven.com/a/forrest_ground_01 |
| `dirt.jpg`, `dirt_n.jpg` | **Ground103**, ambientCG.com | https://ambientcg.com/view?id=Ground103 |
| `rock.jpg`, `rock_n.jpg` | **Rock Face 03** (`rock_face_03`) by Dario Barresi and Rico Cilliers, Poly Haven | https://polyhaven.com/a/rock_face_03 |
| `snow.jpg`, `snow_n.jpg` | **Snow006**, ambientCG.com | https://ambientcg.com/view?id=Snow006 |

What was done to them (`tools/terrain/ground.py`, from the 1K JPG downloads:
ambientCG's `_Color.jpg` and `_NormalGL.jpg`, Poly Haven's `diff` and
`nor_gl`):

- Colour images: their broad light and dark flattened in linear light (so a
  repeat doesn't show from afar), their contrast evened a little, rock's
  brown mostly taken out and cooled toward grey, each levelled to a mean
  brightness that suits the others, and scaled to 512×512.
- Normal maps (OpenGL, +y up the image): scaled to 512×512 and renormalized.

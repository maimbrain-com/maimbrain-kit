-- Shader Stress: Phase 5's adversarial phone run (docs/STUDIO-2.md §9.6).
-- Twenty-four see-through screen-filling planes, each drawn with a shader
-- that passes the checks just under the cost limit (shaders/runaway.wgsl,
-- S13's capped runaway). Unguarded that's ~0.3 s of GPU a frame. The fill
-- budget draws all but about three of them plain each frame; three at the
-- limit are still ~40 ms, so the page's guard (timestamps: 3 frames in a
-- row over 34 ms) puts it on plain materials for good. The HUD shows the
-- frame rate, the share of missed frames and the longest frame, from dt.
-- Shaded layers glow orange; plain ones (past the budget, or after the
-- fallback) are pale blue, so the screen shows which is which.

local LAYERS = 24
local plane = geometry.plane(14, 24)
for i = 1, LAYERS do
  local u = { tint = { 1.4, 0.35 + 0.03 * i, 0.15 }, alpha = 0.07 }
  Mesh{ geometry = plane, material = material{ shader = "runaway", uniforms = u, color = 0x40c0ff, transparent = true, opacity = 0.07, doubleSided = true },
        y = -i * 0.05 }
end
Mesh{ geometry = geometry.box(1, 1, 1), material = material{ color = 0xffc040, roughness = 0.4 }, y = 1.2, vry = 1.5, vrx = 0.7 }
-- Looking straight down through the stack (a plane lies flat).
camera.x, camera.y, camera.z = 0, 5, 0.01
camera:lookAt(0, 0, 0)
camera.fov = 60

local n, missed, worst, acc, fps, shown = 0, 0, 0, 0, 0, 0
function love.update(dt)
  n = n + 1
  acc = acc + dt
  if dt > 1 / 50 then missed = missed + 1 end
  if dt > worst then worst = dt end
  if acc >= 1 then
    fps = n / acc
    shown = missed / n
    n, acc, missed = 0, 0, 0
  end
end

function love.draw()
  love.graphics.setColor(1, 1, 1)
  love.graphics.print(string.format("%.0f fps  %.0f%% missed  worst %.0f ms", fps, shown * 100, worst * 1000), 12, 120)
  love.graphics.print(string.format("%.1f s", round.time or 0), 12, 146)
end

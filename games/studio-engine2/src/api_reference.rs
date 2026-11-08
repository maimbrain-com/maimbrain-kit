//! The reference and the type definitions can't drift from the engine
//! (docs/STUDIO-2.md §10.1 risks 13 and 14): every global, field and method
//! the engine exposes is named in docs/STUDIO-2-API.md and declared in
//! `types/love.d.luau` or `types/studio.d.luau`, and everything those name
//! is in the engine. And the starters type-check against the definitions
//! with `luau-analyze`'s checker (Decided 33: nonstrict, the old solver).
//!
//! What the engine exposes is read from a live VM where Lua can see it
//! (globals, library tables, the method tables of LÖVE's objects and the
//! prelude's), and from the bindings' own tables where only Rust can (a
//! node's properties and methods by kind; the methods of the userdata
//! dispatched by `__namecall`: the camera, flux's tweens, anim8's grids and
//! animations, bump's worlds).
//!
//! Names are written one way in all three places:
//!
//! - `love.graphics.rectangle`, `round.score`, `Sprite` (globals and their fields);
//! - `Image:getWidth` (a method), `Animation.position` (a field of an object);
//! - `Sprite.flipX`, `Sprite:play`, `Sprite.animationFinished` (a node
//!   kind's property, method or signal; what every node has is `Node.x`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::api::atoms::{A, NAMES};
use crate::api::node::{KINDS, has, is_method, is_signal};
use crate::project::{Files, Project};
use crate::vm::{Mode, Vm};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The Lua that lists what a VM's Lua can see, as an error (the one string
/// a VM hands back): run from a controller's press, the one way a game
/// gets a Joystick.
const INVENTORY: &str = r#"
function love.gamepadpressed(joystick)
local STD = {}
for _, k in ipairs({ "assert", "error", "getmetatable", "ipairs", "next", "pairs", "pcall", "rawequal", "rawget", "rawset",
  "rawlen", "select", "setmetatable", "tonumber", "tostring", "type", "typeof", "unpack", "xpcall", "_G", "_VERSION",
  "math", "string", "table", "coroutine", "bit32", "utf8", "buffer", "vector" }) do STD[k] = true end
local out = {}
local function add(s) out[#out + 1] = s end
-- A table's string keys, its own and through __index tables.
local function keys(t)
  local seen, list, cur, depth = {}, {}, t, 0
  while type(cur) == "table" and depth < 4 do
    for k in pairs(cur) do
      if type(k) == "string" and not seen[k] and k:sub(1, 2) ~= "__" then seen[k] = true list[#list + 1] = k end
    end
    local mt = getmetatable(cur)
    cur = type(mt) == "table" and rawget(mt, "__index") or nil
    depth += 1
  end
  table.sort(list)
  return list
end
local function walk(prefix, t, depth)
  for _, k in ipairs(keys(t)) do
    if depth > 0 or not STD[k] then
      local v = t[k]
      local p = prefix .. k
      if v == bit32 then add(p)
      elseif type(v) == "table" and depth < 2 then walk(p .. ".", v, depth + 1)
      elseif type(v) == "function" then add(p)
      else add(p) end
    end
  end
end
walk("", _G, 0)
-- The globals the prelude and the VM set in the game's own global table
-- (which _G, the engine's, reads through to but doesn't list).
local own = {}
OWN_GLOBALS
walk("", own, 0)
-- math: only what the engine replaces.
add("math.random") add("math.randomseed")
local function methods(name, obj)
  local mt = getmetatable(obj)
  local idx = type(mt) == "table" and rawget(mt, "__index")
  if type(idx) == "table" then
    for _, k in ipairs(keys(idx)) do add(name .. ":" .. k) end
  end
end
local g = love.graphics
local img = g.newImage("player")
methods("Image", img)
methods("Quad", g.newQuad(0, 0, 1, 1, 1, 1))
methods("Font", g.getFont())
methods("TextObject", g.newText(g.getFont(), "x"))
methods("SpriteBatch", g.newSpriteBatch(img))
methods("ParticleSystem", g.newParticleSystem("sparkles"))
methods("Source", love.audio.newSource("catch", "static"))
methods("Joystick", joystick)
methods("RandomGenerator", love.math.newRandomGenerator(1))
methods("Canvas", g.newCanvas())
methods("Shader", g.newShader("fn sprite(in: Sprite) -> vec4<f32> { return in.color; }"))
methods("Cursor", love.mouse.getSystemCursor("arrow"))
-- The prelude sets these each frame (input.pad is nil without a controller).
for _, k in ipairs({ "ax", "ay", "pad" }) do add("input." .. k) end
-- The game sets these (the prelude hands them to the engine); nil until then.
add("round.subject") add("round.endDelay")
-- cast()'s handle: its fields go through to the engine (a cast member needs a line to make).
add("CastMember.expression") add("CastMember.health") add("CastMember:hit")
methods("TimerHandle", timer.after(1, function() end))
local s = Sprite{ image = "player" }
methods("Signal", s.animationFinished)
local tw = s:createTween()
methods("Tween", tw)
methods("Tweener", tw:tweenProperty("x", 1, 1))
add("Tween.finished")
s:destroy()
methods("Hint", ui.hint{ text = "TAP" })
methods("Lesson", ui.lesson{ prompt = "TAP" })
THREE_D
error(table.concat(out, "\n"), 0)
end
"#;

/// What a 3D game's VM adds (run inside INVENTORY's function): 1.0's
/// objects' methods, and what the 3D prelude sets each frame or serves
/// through the globals' metatable.
const INVENTORY_3D: &str = r#"
methods("Geometry", mesh.box())
methods("Material", material{})
methods("Emitter3D", emitter{})
methods("Voice", loop("catch"))
for _, k in ipairs({ "x", "y", "down", "jump" }) do add("input." .. k) end
for _, k in ipairs({ "score", "lives", "title", "over" }) do add(k) end
"#;

/// The catch starter with `main` as its main.lua, compiled (as a 3D game,
/// with a model, for `three`).
fn files_with(main: &str, three: bool) -> Files {
    let mut f = Files::new();
    let dir = root().join("starters/catch");
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(b) = std::fs::read(&p) {
                f.insert(p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/"), b);
            }
        }
    }
    if three {
        let toml = String::from_utf8(f["studio.toml"].clone()).unwrap().replace("kind = \"2d\"", "kind = \"3d\"");
        f.insert("studio.toml".into(), toml.into_bytes());
        f.insert("models/fox.glb".into(), b"glTF".to_vec());
    }
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    let bc = luau::compile(main.as_bytes()).unwrap_or_else(|e| panic!("inventory:{}: {}", e.line, e.message));
    f.insert("game/main.luac".into(), bc);
    f
}

/// Globals set in the game's own global table: the prelude's top-level
/// assignments (and the 3D prelude's, in a 3D game), and what the VM sets
/// there (`Object`).
fn own_globals(three: bool) -> Vec<String> {
    let mut out = Vec::new();
    let prelude = if three { [include_str!("prelude.lua"), include_str!("prelude3d.lua")].concat() } else { include_str!("prelude.lua").to_string() };
    let locals: Vec<&str> = prelude.lines().filter_map(|l| l.strip_prefix("local ")).map(|l| l.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).next().unwrap()).collect();
    for line in prelude.lines() {
        let name = if let Some(r) = line.strip_prefix("function ") { r } else { line };
        let id: String = name.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let rest = &name[id.len()..];
        let assigns = (line.starts_with("function ") && (rest.starts_with('(') || rest.starts_with('.') || rest.starts_with(':'))) || rest.starts_with(" = ");
        if !id.is_empty() && !locals.contains(&id.as_str()) && id != "local" && id != "return" && assigns && !out.contains(&id) {
            out.push(id);
        }
    }
    let vm = include_str!("vm.rs");
    for part in vm.split("setfield(t, LUA_GLOBALSINDEX, c\"").skip(1) {
        out.push(part.split('"').next().unwrap().to_string());
    }
    out
}

/// What Lua sees in a 2D or a 3D game, one name a line.
fn lua_inventory(three: bool) -> BTreeSet<String> {
    let list = own_globals(three).iter().map(|g| format!("own[{g:?}] = {g}")).collect::<Vec<_>>().join("\n");
    let files = files_with(&INVENTORY.replace("OWN_GLOBALS", &list).replace("THREE_D", if three { INVENTORY_3D } else { "" }), three);
    let p = Rc::new(Project::parse(&files).unwrap());
    let images: Vec<_> = p.images.iter().map(crate::scene_image).collect();
    let mut v = Vm::new(&p, 1, Mode::Round, &images, &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap_or_else(|e| panic!("the inventory's VM: {e}"));
    match v.step(1.0 / 60.0, &[crate::input::LoveEvent::GamepadPressed("a")], &crate::input::Held::default(), false, false) {
        Ok(_) => panic!("the inventory didn't report"),
        Err(e) if e.contains("game/main.lua:") => panic!("the inventory failed: {e}"),
        Err(e) => e.lines().map(str::to_string).collect(),
    }
}

/// `"x" =>` and `"a" | "b" =>` arms (string dispatch) and `Some(A::X)`
/// arms (atom dispatch) in the binding `name`'s body in `src`.
fn arms(src: &str, name: &str) -> BTreeSet<String> {
    let start = src.find(&format!("b!({name},")).unwrap_or_else(|| panic!("no binding {name}"));
    let body = &src[start..];
    let body = &body[..body.find("\n});").expect("the binding's end")];
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with('"') && t.contains("=>") {
            let pat = &t[..t.find("=>").unwrap()];
            for part in pat.split('|') {
                let p = part.trim();
                if let Some(s) = p.strip_prefix('"').and_then(|p| p.split('"').next()) {
                    out.insert(s.to_string());
                }
            }
        }
        if let Some(i) = t.find("Some(A::") {
            let id = t[i + 8..].split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap();
            if let Some(a) = atom_named(id) {
                out.insert(a.name().to_string());
            }
        }
    }
    out
}

/// An atom by its Rust name (`LookAt`).
fn atom_named(id: &str) -> Option<A> {
    (0..NAMES.len() as i32).filter_map(A::from).find(|a| format!("{a:?}") == id)
}

/// Everything the engine exposes, 2D and 3D, as named above.
pub fn engine_inventory() -> BTreeSet<String> {
    let mut out = lua_inventory(false);
    out.extend(lua_inventory(true));
    // 3D nodes: 1.0's methods and the properties every node has.
    for a in (0..NAMES.len() as i32).filter_map(A::from) {
        if crate::api::three::is_prop(a) {
            out.insert(format!("Node3D.{}", a.name()));
        }
        if crate::api::three::is_method(a) {
            out.insert(format!("Node3D:{}", a.name()));
        }
    }
    // 2.0's 3D nodes: what every one has (as `Node3D`), and each kind's own.
    {
        use crate::api::nodes3d::{TYPES3, cam_method, cam_prop, has as has3, is_method as is_method3, is_shared};
        use crate::three::Kind3;
        for a in (0..NAMES.len() as i32).filter_map(A::from) {
            let sep = if is_method3(a) { ":" } else { "." };
            if is_shared(a) || has3(Kind3::Group, a) {
                out.insert(format!("Node3D{sep}{}", a.name()));
                continue;
            }
            for (name, k) in TYPES3 {
                if has3(*k, a) {
                    out.insert(format!("{name}{sep}{}", a.name()));
                }
            }
            if cam_prop(a) {
                out.insert(format!("camera.{}", a.name()));
            }
            if cam_method(a) {
                out.insert(format!("camera:{}", a.name()));
            }
        }
        for n in ["AnimationTree.state", "AnimationTree.model", "AnimationTree:travel", "AnimationTree:set", "AnimationTree:get"] {
            out.insert(n.into());
        }
        // A CharacterBody2D is a node whose members come from what it is
        // (character2d.rs), not its kind.
        for a in [A::FloorMaxAngle, A::FloorSnapLength, A::MaxStepHeight, A::FloorStopOnSlope, A::FloorBlockOnWall, A::WallMinSlideAngle, A::SlideOnCeiling] {
            out.insert(format!("CharacterBody2D.{}", a.name()));
        }
        for a in [
            A::MoveAndSlide,
            A::IsOnFloor,
            A::IsOnFloorOnly,
            A::IsOnWall,
            A::IsOnWallOnly,
            A::IsOnCeiling,
            A::GetFloorNormal,
            A::GetWallNormal,
            A::GetFloorAngle,
            A::GetRealVelocity,
            A::GetSlideCollisionCount,
            A::GetSlideCollision,
        ] {
            out.insert(format!("CharacterBody2D:{}", a.name()));
        }
    }
    // `audio.bus`'s properties (api/sound.rs).
    for n in ["name", "volume", "lowpass", "highpass", "reverb", "delay"] {
        out.insert(format!("Bus.{n}"));
    }
    // Nodes: what every node has (as `Node.`), and each kind's own.
    let atoms: Vec<A> = (0..NAMES.len() as i32).filter_map(A::from).collect();
    let all = |a: A| KINDS.iter().all(|(_, k)| has(*k, a));
    for &a in &atoms {
        let sep = if is_method(a) { ":" } else { "." };
        if all(a) {
            out.insert(format!("Node{sep}{}", a.name()));
            continue;
        }
        for (name, k) in KINDS {
            if has(*k, a) {
                let _ = is_signal(a);
                out.insert(format!("{}{sep}{}", name.to_str().unwrap(), a.name()));
            }
        }
    }
    // LÖVE's callbacks the prelude calls: `call("update", dt)`, and the
    // events its `deliver` passes on by name (`name == "mousepressed"`).
    let prelude = include_str!("prelude.lua");
    let deliver = prelude.split("local function deliver(").nth(1).expect("the prelude's deliver");
    let deliver = &deliver[..deliver.find("\nend").expect("deliver's end")];
    for part in prelude.split("call(\"").skip(1).chain(deliver.split("name == \"").skip(1)) {
        out.insert(format!("love.{}", part.split('"').next().unwrap()));
    }
    // Node methods written in Lua (the prelude's nodeMethods).
    let block = &prelude[prelude.find("local nodeMethods = {").expect("nodeMethods")..];
    for line in block.lines().skip(1).take_while(|l| !l.starts_with('}')) {
        if let Some((k, _)) = line.trim().split_once(" = function") {
            out.insert(format!("Node:{k}"));
            out.insert(format!("Node3D:{k}"));
        }
    }
    // Userdata dispatched by __namecall.
    let node = include_str!("api/node.rs");
    let libs = include_str!("api/libs.rs");
    for f in arms(node, "c_index") {
        out.insert(format!("camera.{f}"));
    }
    for m in arms(node, "c_namecall") {
        // camera:shake answers with fx.shake: not the camera's.
        if m != "shake" {
            out.insert(format!("camera:{m}"));
        }
    }
    for (kind, f) in [("FluxTween", "tween_call"), ("Grid", "grid_namecall"), ("Animation", "anim_namecall"), ("World", "world_call")] {
        for m in arms(libs, f) {
            out.insert(format!("{kind}:{m}"));
        }
    }
    for f in arms(libs, "anim_index") {
        out.insert(format!("Animation.{f}"));
    }
    // sensors.scope's handle.
    let sensors = include_str!("api/sensors.rs");
    for f in arms(sensors, "scope_index") {
        out.insert(format!("Scope.{f}"));
    }
    for m in arms(sensors, "scope_namecall") {
        out.insert(format!("Scope:{m}"));
    }
    // Widgets (ui.button, …): their own properties.
    for &a in &atoms {
        if has(maimbrain::scene::Kind::Widget, a) && !all(a) && !is_method(a) {
            out.insert(format!("Widget.{}", a.name()));
        }
    }
    // A combo (`ui.combo`): fields by __index, methods by __namecall.
    let ui = include_str!("api/ui.rs");
    for f in arms(ui, "combo_index") {
        out.insert(format!("Combo.{f}"));
    }
    for m in arms(ui, "combo_call") {
        out.insert(format!("Combo:{m}"));
    }
    // love.physics's objects (dispatched by __namecall, lovephysics.rs),
    // under names that don't clash with bump's World or the Shape node.
    {
        use crate::api::lovephysics::{BODY_METHODS, CONTACT_METHODS, FIXTURE_METHODS, JOINT_METHODS, SHAPE_METHODS, WORLD_METHODS};
        for (kind, list) in [
            ("PhysicsWorld", WORLD_METHODS),
            ("Body", BODY_METHODS),
            ("Fixture", FIXTURE_METHODS),
            ("PhysicsShape", SHAPE_METHODS),
            ("Joint", JOINT_METHODS),
            ("Contact", CONTACT_METHODS),
        ] {
            for m in list.iter().chain(&["type", "typeOf", "release"]) {
                out.insert(format!("{kind}:{m}"));
            }
        }
    }
    // What the music is doing, read through its metatable (api/music.rs).
    for f in arms(include_str!("api/music.rs"), "m_index") {
        out.insert(format!("music.{f}"));
    }
    out
}

/// What a type definitions file declares, named as above: `declare x: {…}`
/// blocks by path, `declare function F`, and `declare extern type T with
/// … end` as `T.field` and `T:method` (`Camera` is the global `camera`,
/// and so on for the globals declared as a type).
pub fn declared(src: &str) -> BTreeSet<String> {
    enum State {
        Top,
        Type(String),
        Table(Vec<String>),
        Skip,
    }
    let mut out = BTreeSet::new();
    let mut state = State::Top;
    let ident = |s: &str| -> String { s.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect() };
    for raw in src.lines() {
        let line = raw.split("--").next().unwrap();
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        state = match state {
            State::Skip if raw.starts_with('}') || raw.starts_with("declare ") || raw.starts_with("type ") => State::Top,
            State::Skip => State::Skip,
            s => s,
        };
        match &mut state {
            State::Skip => {}
            State::Top => {
                if let Some(r) = t.strip_prefix("declare extern type ") {
                    state = State::Type(ident(r));
                } else if let Some(r) = t.strip_prefix("declare function ") {
                    out.insert(ident(r));
                    if !t.ends_with(')') && !t.contains("):") {
                        state = State::Skip;
                    }
                } else if let Some(r) = t.strip_prefix("declare ") {
                    let name = ident(r);
                    if t.ends_with('{') {
                        state = State::Table(vec![name]);
                    } else if !["HumpTimer", "MusicPlayer", "SaveStore"].iter().any(|ty| t.ends_with(&format!(": {ty}"))) {
                        // (Those are tables: their fields are the names.)
                        out.insert(name);
                    }
                } else if t.starts_with("type ") && t.ends_with('{') {
                    state = State::Skip;
                }
            }
            State::Type(name) => {
                // The globals declared as these types.
                let prefix = match name.as_str() {
                    "Camera" => "camera",
                    "HumpTimer" => "timer",
                    "MusicPlayer" => "music",
                    "SaveStore" => "save",
                    n => n,
                }
                .to_string();
                if t == "end" {
                    state = State::Top;
                } else if t.starts_with("function __") {
                    // A metamethod (`camera(…)`): the global itself.
                } else if let Some(r) = t.strip_prefix("function ") {
                    out.insert(format!("{prefix}:{}", ident(r)));
                } else if t.contains(':') {
                    out.insert(format!("{prefix}.{}", ident(t)));
                }
            }
            State::Table(path) => {
                if t.starts_with('}') {
                    path.pop();
                    if path.is_empty() {
                        state = State::Top;
                    }
                } else if t.contains(':') {
                    let key = ident(t);
                    if t.ends_with('{') {
                        path.push(key);
                    } else {
                        out.insert(format!("{}.{key}", path.join(".")));
                    }
                }
            }
        }
    }
    out
}

/// docs/STUDIO-2-API.md's names: each page's `<!-- names: … -->` list,
/// with the page's text (without the list) to check each is described.
pub fn documented(doc: &str) -> Vec<(String, String, BTreeSet<String>)> {
    let mut pages = Vec::new();
    for page in doc.split("\n## ").skip(1) {
        let title = page.lines().next().unwrap_or("").to_string();
        let mut names = BTreeSet::new();
        let mut text = String::new();
        let mut rest = page;
        while let Some(i) = rest.find("<!-- names:") {
            text.push_str(&rest[..i]);
            let end = rest[i..].find("-->").map_or(rest.len(), |e| i + e + 3);
            names.extend(rest[i + 11..end - 3].split_whitespace().map(str::to_string));
            rest = &rest[end..];
        }
        text.push_str(rest);
        pages.push((title, text, names));
    }
    pages
}

/// Luau's own names the engine replaces or adds to: the checker already
/// knows their types.
const BUILTINS: &[&str] = &["print", "require", "math.random", "math.randomseed"];

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn doc() -> String {
    read("../../docs/STUDIO-2-API.md")
}

fn list(what: &str, names: &BTreeSet<String>) -> String {
    format!("{what} ({}):\n  {}", names.len(), names.iter().cloned().collect::<Vec<_>>().join("\n  "))
}

/// Everything the engine exposes is in the type definitions, and
/// everything they declare is in the engine (risk 14).
#[test]
fn the_type_definitions_declare_what_the_engine_has() {
    let engine = engine_inventory();
    let mut types = declared(&read("types/love.d.luau"));
    types.extend(declared(&read("types/studio.d.luau")));
    types.extend(BUILTINS.iter().map(|s| s.to_string()));
    // A 3D node's `parent` is a property and engine 1.0's method; a type
    // has one member a name, the method (what 1.0's code calls).
    types.insert("Node3D.parent".into());
    let missing: BTreeSet<_> = engine.difference(&types).cloned().collect();
    let extra: BTreeSet<_> = types.difference(&engine).cloned().collect();
    assert!(missing.is_empty() && extra.is_empty(), "{}\n{}", list("in the engine, not in types/*.d.luau", &missing), list("in types/*.d.luau, not in the engine", &extra));
}

/// Everything the engine exposes is in the reference, on a page that
/// describes it, and the reference names nothing the engine hasn't (risk 13).
#[test]
fn the_reference_names_what_the_engine_has() {
    let engine = engine_inventory();
    let pages = documented(&doc());
    let mut named = BTreeSet::new();
    let mut undescribed = BTreeSet::new();
    for (title, text, names) in &pages {
        for n in names {
            let short = n.rsplit(['.', ':']).next().unwrap();
            if !text.contains(short) {
                undescribed.insert(format!("{n} (on \"{title}\")"));
            }
        }
        named.extend(names.iter().cloned());
    }
    let missing: BTreeSet<_> = engine.difference(&named).cloned().collect();
    let extra: BTreeSet<_> = named.difference(&engine).cloned().collect();
    assert!(
        missing.is_empty() && extra.is_empty() && undescribed.is_empty(),
        "{}\n{}\n{}",
        list("in the engine, not in docs/STUDIO-2-API.md", &missing),
        list("in docs/STUDIO-2-API.md, not in the engine", &extra),
        list("named on a page that doesn't mention them", &undescribed)
    );
}

/// A project's Lua files, as luau-analyze gets them.
fn lua_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("game")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "lua") {
                out.push((p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn check(files: &[(String, String)]) -> Vec<luau::TypeError> {
    let (love, studio) = (read("types/love.d.luau"), read("types/studio.d.luau"));
    let defs = [("love.d.luau", love.as_str()), ("studio.d.luau", studio.as_str())];
    let files: Vec<(&str, &str)> = files.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect();
    luau::analyze(&defs, &files)
}

/// Every starter and example type-checks against the definitions with no errors
/// (Decided 33's mode: the old solver, nonstrict).
#[test]
fn the_starters_type_check() {
    let mut starters: Vec<PathBuf> = ["starters", "examples"]
        .iter()
        .flat_map(|d| std::fs::read_dir(root().join(d)).into_iter().flatten().flatten().map(|e| e.path()))
        .filter(|p| p.join("studio.toml").is_file())
        .collect();
    starters.sort();
    assert!(!starters.is_empty());
    for s in starters {
        let errors = check(&lua_files(&s));
        assert!(errors.is_empty(), "{}: {errors:#?}", s.display());
    }
}

/// Engine 1.0's tunnel on the 3D build type-checks against 2.0's
/// definitions but for two reports (never blocking, Decided 33): a haptic
/// kind picked with `and`/`or` is a string to the checker, and `AUTO = AUTO
/// or false` reads a global before it's assigned.
#[test]
fn engine_1_0s_3d_lua_type_checks_but_for_two_reports() {
    let errors = check(&lua_files(&root().join("compat/tunnel-1.0")));
    let lines: Vec<u32> = errors.iter().map(|e| e.line).collect();
    assert_eq!(lines, [274, 301], "{errors:#?}");
    assert!(errors[0].message.contains("\"tap\" | string") && errors[1].message.contains("Unknown global 'AUTO'"), "{errors:#?}");
}

/// 1.0's names that are also 2.0 tables (`timer`, `music`, `save`, the
/// camera) and 1.0's `spawn` are callable to the checker too.
#[test]
fn engine_1_0s_calls_type_check() {
    let src = "timer(\"wave\", 1, true)\ntimer.after(1, function() end)\nmusic(\"theme\", 0.5)\nmusic.stop()\nsave(\"k\", 1)\nlocal v = save.get(\"k\")\ncamera(0, 1, 5, 0, 0, 0, 0.9)\nlocal fox = spawn(model(\"fox\"))\nfox:place(1, 2, 3)\nlocal n = node(mesh.box(1, 1, 1):color(0xff0000ff), material{ rough = 1 })\nn.z = -2\nscore = score + 1\nif not over then game_over(\"X\") end\n";
    let errors = check(&[("game/main.lua".into(), src.into())]);
    assert!(errors.is_empty(), "{errors:#?}");
}

/// The definitions catch the mistakes they're for: misspelled API names, a
/// wrong enum string, a string for a number, a misspelled method. Each with
/// its file, line and column. (Nonstrict mode types a node made into an
/// unannotated local as `any`, so a misspelled property on it, or a key in
/// its constructor's table, isn't caught here; the engine names it, with
/// "did you mean", when the line runs.)
#[test]
fn the_definitions_catch_mistakes() {
    let src = "function love.load()\n  local s = Sprite{ image = \"hero\" }\n  fx.shaek(4, 0.2)\nend\nfunction love.draw()\n  love.graphics.rectangel(\"fill\", 0, 0, 10, 10)\n  love.graphics.circle(\"filled\", 0, 0, 5)\n  love.graphics.print(\"hi\", \"10\", 20)\n  camera:lookat(1, 2)\nend\n";
    let errors = check(&[("game/main.lua".into(), src.into())]);
    let lines: Vec<u32> = errors.iter().map(|e| e.line).collect();
    assert_eq!(lines, [3, 6, 7, 8, 9], "{errors:#?}");
    assert!(errors.iter().all(|e| e.file == "game/main.lua" && e.column > 0), "{errors:#?}");
}

/// docs/STUDIO-2.md §6.1 shows Lantern Path's files as the example has
/// them (games/studio-engine2/examples/lantern-path): its studio.toml, its
/// map and its main.lua, in that order.
#[test]
fn lantern_path_is_the_design_s_code() {
    // The design is the monorepo's; the kit ships the engine without it.
    if !root().join("../../docs/STUDIO-2.md").is_file() {
        eprintln!("skipped: no docs/STUDIO-2.md here (the creator kit)");
        return;
    }
    let design = read("../../docs/STUDIO-2.md");
    let at = design.find("### 6.1 Lantern Path").expect("§6.1");
    let section = &design[at..design[at..].find("### 6.2").map_or(design.len(), |e| at + e)];
    let blocks: Vec<&str> = section.split("```").skip(1).step_by(2).map(|b| b.split_once('\n').map_or("", |(_, rest)| rest)).collect();
    let files = ["examples/lantern-path/studio.toml", "examples/lantern-path/maps/path1.map", "examples/lantern-path/game/main.lua"];
    assert_eq!(blocks.len(), files.len(), "§6.1's code blocks");
    for (block, file) in blocks.iter().zip(files) {
        assert_eq!(*block, read(file), "§6.1 and {file} differ");
    }
}

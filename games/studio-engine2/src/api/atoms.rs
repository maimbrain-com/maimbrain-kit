//! Names Luau interns as atoms (its `useratom` callback), so a node's
//! property or method is matched by a small number, not a string compare
//! (docs/STUDIO-2.md §4.3). Every property and method name of the nodes (2D
//! and 3D) and the camera is here; any other string gets no atom.

use std::ffi::c_char;

macro_rules! atoms {
    ($($id:ident = $name:literal),* $(,)?) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(i16)]
        pub enum A { $($id),* }
        pub const NAMES: &[&str] = &[$($name),*];
        const ALL: &[A] = &[$(A::$id),*];
        impl A {
            pub fn from(atom: i32) -> Option<A> {
                ALL.get(usize::try_from(atom).ok()?).copied()
            }
            pub fn name(self) -> &'static str {
                NAMES[self as usize]
            }
        }
        fn lookup(s: &[u8]) -> i16 {
            match std::str::from_utf8(s) {
                $(Ok($name) => A::$id as i16,)*
                _ => -1,
            }
        }
    };
}

atoms! {
    // every node
    X = "x", Y = "y", Rotation = "rotation", Scale = "scale", Sx = "sx", Sy = "sy",
    Vx = "vx", Vy = "vy", Gravity = "gravity", Vr = "vr", Alpha = "alpha", Visible = "visible", Color = "color",
    Order = "order", Fixed = "fixed", Parent = "parent", Children = "children", Name = "name", Tag = "tag",
    W = "w", H = "h", Radius = "radius", Data = "data", Alive = "alive",
    // Sprite and Layer
    Image = "image", Frame = "frame", AnchorX = "anchorX", AnchorY = "anchorY", FlipX = "flipX", FlipY = "flipY",
    // Shape
    Shape = "shape", Points = "points", Corner = "corner", Line = "line",
    // Text
    Text = "text", Font = "font", Size = "size", Align = "align", Width = "width",
    // Layer
    Parallax = "parallax", RepeatX = "repeatX", RepeatY = "repeatY", Fill = "fill",
    // node methods
    Destroy = "destroy", Find = "find", WorldPosition = "worldPosition", Play = "play", Emit = "emit",
    SolidAt = "solidAt", TileAt = "tileAt", Marker = "marker",
    // signals
    AnimationFinished = "animationFinished", Stepped = "stepped",
    // Tilemap, Walker, Emitter
    Behaviour = "behaviour", Behaviours = "behaviours", Knobs = "knobs",
    Map = "map", Tiles = "tiles", Height = "height", Speed = "speed",
    Preset = "preset", Rate = "rate", Colors = "colors", Life = "life", Spread = "spread", Additive = "additive", Emitting = "emitting",
    // the camera
    Rot = "rot", LookAt = "lookAt", Move = "move", ZoomTo = "zoomTo", RotateTo = "rotateTo",
    Attach = "attach", Detach = "detach", WorldCoords = "worldCoords", CameraCoords = "cameraCoords",
    Shake = "shake", Follow = "follow",
    // 3D nodes (engine 1.0's 3D names, api/three.rs)
    Z = "z", Rx = "rx", Ry = "ry", Rz = "rz", Sz = "sz", Vz = "vz", Vrx = "vrx", Vry = "vry", Vrz = "vrz",
    Place = "place", Pos = "pos", Turn = "turn", Show = "show", World = "world", Fade = "fade",
    Light = "light", Instances = "instances", Material = "material", Clip = "clip",
    // 3D nodes (three.js's and Godot's names, api/nodes3d.rs)
    Geometry = "geometry", Model = "model", Intensity = "intensity", Range = "range", Billboard = "billboard",
    Wake = "wake", FaceDirection = "faceDirection", Bone = "bone", Reach = "reach", HeadLook = "headLook",
    PlantFeet = "plantFeet", Gait = "gait", Flight = "flight", Swim = "swim", Restyle = "restyle",
    ClipTime = "clipTime", Instance = "instance", Length = "length", Sizes = "sizes", Drag = "drag",
    // the 3D camera
    Fov = "fov", Roll = "roll", Project = "project", Ray = "ray", Near = "near", Far = "far",
    // AnimationTree
    State = "state", Travel = "travel", Set = "set", Get = "get",
    // Terrain
    Progress = "progress", Built = "built", HeightAt = "heightAt", NormalAt = "normalAt", Raise = "raise",
    // foley (api/sound.rs)
    Footsteps = "footsteps", Hum = "hum",
    // love.physics: the body a 2D node follows
    Body = "body",
    // 3D physics: Godot's bodies, areas, vehicles and joints (bodies3d.rs)
    Mass = "mass", Friction = "friction", Bounce = "bounce", GravityScale = "gravityScale", LinearDamp = "linearDamp",
    AngularDamp = "angularDamp", ContactMonitor = "contactMonitor", Sleeping = "sleeping", Freeze = "freeze",
    CanSleep = "canSleep", ContinuousCd = "continuousCd", LockRotation = "lockRotation", CollisionLayer = "collisionLayer",
    CollisionMask = "collisionMask", Monitoring = "monitoring",
    ApplyImpulse = "applyImpulse", ApplyCentralImpulse = "applyCentralImpulse", ApplyForce = "applyForce",
    ApplyCentralForce = "applyCentralForce", ApplyTorque = "applyTorque", ApplyTorqueImpulse = "applyTorqueImpulse",
    GetCollidingBodies = "getCollidingBodies", GetOverlappingBodies = "getOverlappingBodies",
    GetOverlappingAreas = "getOverlappingAreas", OverlapsBody = "overlapsBody",
    BodyEntered = "bodyEntered", BodyExited = "bodyExited", AreaEntered = "areaEntered", AreaExited = "areaExited",
    EngineForce = "engineForce", Brake = "brake", Steering = "steering",
    UseAsTraction = "useAsTraction", UseAsSteering = "useAsSteering", WheelRadius = "wheelRadius",
    WheelRestLength = "wheelRestLength", SuspensionTravel = "suspensionTravel", SuspensionStiffness = "suspensionStiffness",
    SuspensionMaxForce = "suspensionMaxForce", DampingCompression = "dampingCompression",
    DampingRelaxation = "dampingRelaxation", WheelFrictionSlip = "wheelFrictionSlip", IsInContact = "isInContact",
    NodeA = "nodeA", NodeB = "nodeB", BreakForce = "breakForce", BreakTorque = "breakTorque", Force = "force",
    Torque = "torque", LimitLower = "limitLower", LimitUpper = "limitUpper", MotorVelocity = "motorVelocity",
    MotorMaxImpulse = "motorMaxImpulse", Broken = "broken",
    // Widgets (ui.button, ui.panel, …: api/ui.rs)
    Value = "value", Icon = "icon", Enabled = "enabled", Tap = "tap", Tone = "tone", ButtonKind = "kind", Focus = "focus",
    // Characters, 3D and 2D (bodies3d.rs, character2d.rs): Godot's CharacterBody3D and 2D
    FloorMaxAngle = "floorMaxAngle", FloorSnapLength = "floorSnapLength", MaxStepHeight = "maxStepHeight",
    FloorStopOnSlope = "floorStopOnSlope", FloorBlockOnWall = "floorBlockOnWall", WallMinSlideAngle = "wallMinSlideAngle",
    SlideOnCeiling = "slideOnCeiling", MoveAndSlide = "moveAndSlide", IsOnFloor = "isOnFloor", IsOnFloorOnly = "isOnFloorOnly",
    IsOnWall = "isOnWall", IsOnWallOnly = "isOnWallOnly", IsOnCeiling = "isOnCeiling", GetFloorNormal = "getFloorNormal",
    GetWallNormal = "getWallNormal", GetFloorAngle = "getFloorAngle", GetRealVelocity = "getRealVelocity",
    GetSlideCollisionCount = "getSlideCollisionCount", GetSlideCollision = "getSlideCollision",
    // Navigation (nav3d.rs): Godot's NavigationRegion3D and NavigationAgent3D; Tilemap:path
    Bake = "bake", Baked = "baked", BakeFinished = "bakeFinished", Path = "path",
    MaxSpeed = "maxSpeed", MaxAcceleration = "maxAcceleration", PathDesiredDistance = "pathDesiredDistance",
    TargetDesiredDistance = "targetDesiredDistance", Avoidance = "avoidance", AutoMove = "autoMove",
    SetTargetPosition = "setTargetPosition", GetNextPathPosition = "getNextPathPosition",
    IsNavigationFinished = "isNavigationFinished", IsTargetReached = "isTargetReached", IsTargetReachable = "isTargetReachable",
    DistanceToTarget = "distanceToTarget", GetFinalPosition = "getFinalPosition", TargetReached = "targetReached",
    NavigationFinished = "navigationFinished",
}

/// Luau's `useratom`: called as each string is made.
pub unsafe extern "C" fn useratom(_: *mut luau::lua_State, s: *const c_char, l: usize) -> i16 {
    lookup(unsafe { std::slice::from_raw_parts(s as *const u8, l) })
}

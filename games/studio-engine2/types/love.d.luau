-- Studio engine 2.0's LÖVE and LÖVE's libraries, as type definitions for
-- luau-analyze (docs/STUDIO-2.md §4.13, Decided 33: the old solver,
-- nonstrict mode; reported, never blocking). docs/STUDIO-2-API.md is the
-- reference; studio.d.luau has the rest (nodes, the camera, rounds, fx).
--
-- Only what the engine has: a name that isn't here isn't in the engine.
-- games/studio-engine2's api_reference test keeps the engine, this file,
-- studio.d.luau and the reference page naming the same things. Keep this
-- file's shape for that test: one name a line, `declare` blocks and
-- `declare extern type … with … end`.

-- A colour: 0xRRGGBB, or { r, g, b[, a] } from 0 to 1.
type Color = number | { number }

declare extern type Image with
    function getWidth(self): number
    function getHeight(self): number
    function getDimensions(self): (number, number)
    function setFilter(self, min: "nearest" | "linear", mag: ("nearest" | "linear")?): ()
    function getFilter(self): (string, string)
    function typeOf(self, name: string): boolean
end

declare extern type Quad with
    function getViewport(self): (number, number, number, number)
    function setViewport(self, x: number, y: number, w: number, h: number, sw: number?, sh: number?): ()
    function typeOf(self, name: string): boolean
end

declare extern type Font with
    function getWidth(self, text: string | number): number
    function getHeight(self): number
    function getLineHeight(self): number
    function getWrap(self, text: string | number, limit: number): (number, { string })
    function getAscent(self): number
    function getDescent(self): number
    function getBaseline(self): number
    function setFilter(self, min: string, mag: string?): ()
    function typeOf(self, name: string): boolean
end

-- love.graphics.newText's object (LÖVE's Text).
declare extern type TextObject with
    function set(self, text: string | number): ()
    function setf(self, text: string | number, limit: number, align: ("left" | "center" | "right" | "justify")?): ()
    function clear(self): ()
    function getWidth(self): number
    function getHeight(self): number
    function getDimensions(self): (number, number)
    function typeOf(self, name: string): boolean
end

declare extern type SpriteBatch with
    function add(self, ...: any): number
    function set(self, id: number, ...: any): ()
    function clear(self): ()
    function getCount(self): number
    function setColor(self, r: Color?, g: number?, b: number?, a: number?): ()
    function getImage(self): Image
    function typeOf(self, name: string): boolean
end

declare extern type ParticleSystem with
    function emit(self, count: number?): ()
    function update(self, dt: number?): ()
    function setPosition(self, x: number, y: number): ()
    function moveTo(self, x: number, y: number): ()
    function getPosition(self): (number, number)
    function setEmissionRate(self, rate: number): ()
    function getEmissionRate(self): number
    function setParticleLifetime(self, min: number, max: number?): ()
    function setSpeed(self, min: number, max: number?): ()
    function setSpread(self, angle: number): ()
    function setLinearAcceleration(self, xmin: number, ymin: number?, xmax: number?, ymax: number?): ()
    function setColors(self, ...: any): ()
    function setSizes(self, ...: number): ()
    function start(self): ()
    function stop(self): ()
    function pause(self): ()
    function reset(self): ()
    function isActive(self): boolean
    function getCount(self): number
    function setBufferSize(self, size: number): ()
    function getBufferSize(self): number
    function typeOf(self, name: string): boolean
end

declare extern type Source with
    function play(self): ()
    function stop(self): ()
    function pause(self): ()
    function resume(self): ()
    function clone(self): Source
    function isPlaying(self): boolean
    function isLooping(self): boolean
    function setLooping(self, loop: boolean): ()
    function setVolume(self, volume: number): ()
    function getVolume(self): number
    function setPitch(self, pitch: number): ()
    function getPitch(self): number
    function setPosition(self, x: number, y: number, z: number?): ()
    function getPosition(self): (number, number, number)
    function tell(self, unit: ("seconds" | "samples")?): number
    function seek(self, position: number, unit: ("seconds" | "samples")?): ()
    function getDuration(self, unit: ("seconds" | "samples")?): number
    function typeOf(self, name: string): boolean
end

declare extern type Joystick with
    function isGamepadDown(self, ...: string): boolean
    function getGamepadAxis(self, axis: string): number
    function getName(self): string
    function isConnected(self): boolean
    function getID(self): (number, number)
    function isGamepad(self): boolean
    function getAxis(self, axis: number): number
    function getAxes(self): ...number
    function getAxisCount(self): number
    function typeOf(self, name: string): boolean
end

-- love.graphics.newCanvas: what's drawn into it, kept until clear (no pixels).
declare extern type Canvas with
    function getWidth(self): number
    function getHeight(self): number
    function getDimensions(self): (number, number)
    function renderTo(self, fn: () -> ()): ()
    function setFilter(self, min: string, mag: string?): ()
    function newImageData(self, ...: any): never
    function typeOf(self, name: string): boolean
end

-- love.graphics.newShader: a sprite shader (WGSL with fn sprite, from shaders/<name>.wgsl).
declare extern type Shader with
    function send(self, name: string, value: any): ()
    function hasUniform(self, name: string): boolean
    function typeOf(self, name: string): boolean
end

-- love.mouse.getSystemCursor's: a phone has no pointer, so it does nothing.
declare extern type Cursor with
    function getType(self): string
    function typeOf(self, name: string): boolean
end

declare extern type RandomGenerator with
    function random(self, min: number?, max: number?): number
    function randomNormal(self, stddev: number?, mean: number?): number
    function setSeed(self, seed: number): ()
    function typeOf(self, name: string): boolean
end

-- love.physics (docs/STUDIO-2-API.md [physics]): Box2D's API on Rapier 2D.
-- LÖVE calls these types World and Shape; here they're PhysicsWorld and
-- PhysicsShape, apart from bump's World and the Shape node.
type BodyType = "static" | "dynamic" | "kinematic"

declare extern type PhysicsWorld with
    function update(self, dt: number, velocityiterations: number?, positioniterations: number?): ()
    function setGravity(self, x: number, y: number): ()
    function getGravity(self): (number, number)
    function setCallbacks(self, beginContact: ((a: Fixture, b: Fixture, contact: Contact) -> ())?, endContact: ((a: Fixture, b: Fixture, contact: Contact) -> ())?, preSolve: ((a: Fixture, b: Fixture, contact: Contact) -> ())?, postSolve: ((a: Fixture, b: Fixture, contact: Contact, normalImpulse: number, tangentImpulse: number) -> ())?): ()
    function getCallbacks(self): (any, any, any, any)
    function rayCast(self, x1: number, y1: number, x2: number, y2: number, callback: (fixture: Fixture, x: number, y: number, xn: number, yn: number, fraction: number) -> number): ()
    function queryBoundingBox(self, x1: number, y1: number, x2: number, y2: number, callback: (fixture: Fixture) -> boolean): ()
    function getBodies(self): { Body }
    function getBodyCount(self): number
    function getJoints(self): { Joint }
    function getJointCount(self): number
    function getContacts(self): { Contact }
    function getContactCount(self): number
    function setSleepingAllowed(self, allowed: boolean): ()
    function isSleepingAllowed(self): boolean
    function isLocked(self): boolean
    function destroy(self): ()
    function isDestroyed(self): boolean
    function type(self): string
    function typeOf(self, name: string): boolean
    function release(self): ()
end

declare extern type Body with
    function getPosition(self): (number, number)
    function getX(self): number
    function getY(self): number
    function getAngle(self): number
    function setPosition(self, x: number, y: number): ()
    function setX(self, x: number): ()
    function setY(self, y: number): ()
    function setAngle(self, angle: number): ()
    function getTransform(self): (number, number, number)
    function setTransform(self, x: number, y: number, angle: number?): ()
    function getLinearVelocity(self): (number, number)
    function setLinearVelocity(self, x: number, y: number): ()
    function getAngularVelocity(self): number
    function setAngularVelocity(self, w: number): ()
    function applyForce(self, fx: number, fy: number, x: number?, y: number?): ()
    function applyLinearImpulse(self, ix: number, iy: number, x: number?, y: number?): ()
    function applyTorque(self, torque: number): ()
    function applyAngularImpulse(self, impulse: number): ()
    function getMass(self): number
    function setMass(self, mass: number): ()
    function getInertia(self): number
    function resetMassData(self): ()
    function getMassData(self): (number, number, number, number)
    function getType(self): BodyType
    function setType(self, kind: BodyType): ()
    function isAwake(self): boolean
    function setAwake(self, awake: boolean): ()
    function isSleepingAllowed(self): boolean
    function setSleepingAllowed(self, allowed: boolean): ()
    function isFixedRotation(self): boolean
    function setFixedRotation(self, fixed: boolean): ()
    function isBullet(self): boolean
    function setBullet(self, bullet: boolean): ()
    function getGravityScale(self): number
    function setGravityScale(self, scale: number): ()
    function getLinearDamping(self): number
    function setLinearDamping(self, damping: number): ()
    function getAngularDamping(self): number
    function setAngularDamping(self, damping: number): ()
    function getWorldPoint(self, x: number, y: number): (number, number)
    function getWorldPoints(self, ...: number): ...number
    function getLocalPoint(self, x: number, y: number): (number, number)
    function getWorldVector(self, x: number, y: number): (number, number)
    function getLocalVector(self, x: number, y: number): (number, number)
    function getWorldCenter(self): (number, number)
    function getLocalCenter(self): (number, number)
    function getLinearVelocityFromWorldPoint(self, x: number, y: number): (number, number)
    function getLinearVelocityFromLocalPoint(self, x: number, y: number): (number, number)
    function getFixtures(self): { Fixture }
    function getJoints(self): { Joint }
    function getContacts(self): { Contact }
    function isTouching(self, other: Body): boolean
    function getUserData(self): any
    function setUserData(self, value: any): ()
    function getWorld(self): PhysicsWorld
    function destroy(self): ()
    function isDestroyed(self): boolean
    function isActive(self): boolean
    function setActive(self, active: boolean): ()
    function type(self): string
    function typeOf(self, name: string): boolean
    function release(self): ()
end

declare extern type Fixture with
    function setOneWay(self, on: boolean?): ()
    function isOneWay(self): boolean
    function getBody(self): Body
    function getShape(self): PhysicsShape
    function getType(self): string
    function setFriction(self, friction: number): ()
    function getFriction(self): number
    function setRestitution(self, restitution: number): ()
    function getRestitution(self): number
    function setDensity(self, density: number): ()
    function getDensity(self): number
    function setSensor(self, sensor: boolean): ()
    function isSensor(self): boolean
    function setUserData(self, value: any): ()
    function getUserData(self): any
    function setCategory(self, ...: number): ()
    function getCategory(self): ...number
    function setMask(self, ...: number): ()
    function getMask(self): ...number
    function setGroupIndex(self, group: number): ()
    function getGroupIndex(self): number
    function setFilterData(self, categories: number, mask: number, group: number?): ()
    function getFilterData(self): (number, number, number)
    function testPoint(self, x: number, y: number): boolean
    function getBoundingBox(self): (number, number, number, number)
    function getMassData(self): (number, number, number, number)
    function destroy(self): ()
    function isDestroyed(self): boolean
    function type(self): string
    function typeOf(self, name: string): boolean
    function release(self): ()
end

declare extern type PhysicsShape with
    function getType(self): string
    function getRadius(self): number
    function getPoint(self): (number, number)
    function getPoints(self): ...number
    function getVertexCount(self): number
    function getChildCount(self): number
    function testPoint(self, x: number, y: number, angle: number, px: number, py: number): boolean
    function setRadius(self, radius: number): ()
    function setPoint(self, x: number, y: number): ()
    function type(self): string
    function typeOf(self, name: string): boolean
    function release(self): ()
end

declare extern type Joint with
    function getType(self): string
    function getBodies(self): (Body, Body)
    function getAnchors(self): (number, number, number, number)
    function getReactionForce(self, invdt: number): (number, number)
    function getReactionTorque(self, invdt: number): number
    function getCollideConnected(self): boolean
    function getUserData(self): any
    function setUserData(self, value: any): ()
    function destroy(self): ()
    function isDestroyed(self): boolean
    function setLimits(self, lower: number, upper: number): ()
    function getLimits(self): (number, number)
    function setLimitsEnabled(self, enabled: boolean): ()
    function areLimitsEnabled(self): boolean
    function setLowerLimit(self, lower: number): ()
    function setUpperLimit(self, upper: number): ()
    function getLowerLimit(self): number
    function getUpperLimit(self): number
    function setMotorEnabled(self, enabled: boolean): ()
    function isMotorEnabled(self): boolean
    function setMotorSpeed(self, speed: number): ()
    function getMotorSpeed(self): number
    function setMaxMotorTorque(self, torque: number): ()
    function getMaxMotorTorque(self): number
    function setMaxMotorForce(self, force: number): ()
    function getMaxMotorForce(self): number
    function getJointAngle(self): number
    function getJointTranslation(self): number
    function getJointSpeed(self): number
    function setLength(self, length: number): ()
    function getLength(self): number
    function setMaxLength(self, length: number): ()
    function getMaxLength(self): number
    function setFrequency(self, hz: number): ()
    function getFrequency(self): number
    function setDampingRatio(self, ratio: number): ()
    function getDampingRatio(self): number
    function setSpringFrequency(self, hz: number): ()
    function getSpringFrequency(self): number
    function setSpringDampingRatio(self, ratio: number): ()
    function getSpringDampingRatio(self): number
    function setTarget(self, x: number, y: number): ()
    function getTarget(self): (number, number)
    function setMaxForce(self, force: number): ()
    function getMaxForce(self): number
    function getAxis(self): (number, number)
    function type(self): string
    function typeOf(self, name: string): boolean
    function release(self): ()
end

declare extern type Contact with
    function getPositions(self): ...number
    function getNormal(self): (number, number)
    function getFixtures(self): (Fixture, Fixture)
    function isTouching(self): boolean
    function setEnabled(self, enabled: boolean): ()
    function isEnabled(self): boolean
    function getFriction(self): number
    function setFriction(self, friction: number): ()
    function resetFriction(self): ()
    function getRestitution(self): number
    function setRestitution(self, restitution: number): ()
    function resetRestitution(self): ()
    function getChildren(self): (number, number)
    function type(self): string
    function typeOf(self, name: string): boolean
    function release(self): ()
end

declare love: {
    load: (() -> ())?,
    update: ((dt: number) -> ())?,
    draw: (() -> ())?,
    keypressed: ((key: string, scancode: string, isrepeat: boolean) -> ())?,
    keyreleased: ((key: string, scancode: string) -> ())?,
    mousepressed: ((x: number, y: number, button: number, istouch: boolean, presses: number) -> ())?,
    mousereleased: ((x: number, y: number, button: number, istouch: boolean, presses: number) -> ())?,
    mousemoved: ((x: number, y: number, dx: number, dy: number, istouch: boolean) -> ())?,
    wheelmoved: ((x: number, y: number) -> ())?,
    touchpressed: ((id: number, x: number, y: number, dx: number, dy: number, pressure: number) -> ())?,
    touchmoved: ((id: number, x: number, y: number, dx: number, dy: number, pressure: number) -> ())?,
    touchreleased: ((id: number, x: number, y: number, dx: number, dy: number, pressure: number) -> ())?,
    gamepadpressed: ((joystick: Joystick, button: string) -> ())?,
    gamepadreleased: ((joystick: Joystick, button: string) -> ())?,
    gamepadaxis: ((joystick: Joystick, axis: string, value: number) -> ())?,
    resize: ((w: number, h: number) -> ())?,
    graphics: {
        arc: (mode: "fill" | "line", ...any) -> (),
        circle: (mode: "fill" | "line", x: number, y: number, radius: number, segments: number?) -> (),
        clear: (r: Color?, g: number?, b: number?, a: number?) -> (),
        draw: (drawable: Image | TextObject | SpriteBatch | ParticleSystem | Canvas, ...any) -> (),
        ellipse: (mode: "fill" | "line", x: number, y: number, rx: number, ry: number, segments: number?) -> (),
        getBackgroundColor: () -> (number, number, number, number),
        getCanvas: () -> Canvas?,
        getColor: () -> (number, number, number, number),
        getDefaultFilter: () -> (string, string),
        getDimensions: () -> (number, number),
        getFont: () -> Font,
        getHeight: () -> number,
        getLineWidth: () -> number,
        getWidth: () -> number,
        line: (...any) -> (),
        newCanvas: (w: number?, h: number?, settings: { [string]: any }?) -> Canvas,
        newFont: (...any) -> Font,
        newImage: (name: string) -> Image,
        newParticleSystem: (preset: string | Image, buffer: number?) -> ParticleSystem,
        newQuad: (x: number, y: number, w: number, h: number, sw: number | Image, sh: number?) -> Quad,
        newShader: (nameOrCode: string) -> Shader,
        newSpriteBatch: (image: Image, max: number?) -> SpriteBatch,
        newText: (font: Font, text: (string | number)?) -> TextObject,
        origin: () -> (),
        points: (...any) -> (),
        polygon: (mode: "fill" | "line", ...any) -> (),
        pop: () -> (),
        print: (text: string | number, x: number?, y: number?, r: number?, sx: number?, sy: number?, ox: number?, oy: number?) -> (),
        printf: (text: string | number, x: number, y: number, limit: number, align: ("left" | "center" | "right" | "justify")?, r: number?, sx: number?, sy: number?, ox: number?, oy: number?) -> (),
        push: () -> (),
        rectangle: (mode: "fill" | "line", x: number, y: number, w: number, h: number, rx: number?, ry: number?, segments: number?) -> (),
        rotate: (angle: number) -> (),
        scale: (sx: number, sy: number?) -> (),
        setBackgroundColor: (r: Color, g: number?, b: number?, a: number?) -> (),
        setBlendMode: (mode: "alpha" | "add" | "multiply" | "screen", alphamode: string?) -> (),
        setCanvas: (canvas: (Canvas | { any })?) -> (),
        setShader: (shader: Shader?) -> (),
        getShader: () -> Shader?,
        setColor: (r: Color, g: number?, b: number?, a: number?) -> (),
        setDefaultFilter: (min: "nearest" | "linear", mag: ("nearest" | "linear")?, anisotropy: number?) -> (),
        setFont: (font: Font, ...any) -> (),
        setLineWidth: (width: number) -> (),
        setNewFont: (...any) -> Font,
        setPointSize: (size: number) -> (),
        translate: (dx: number, dy: number) -> (),
    },
    physics: {
        getMeter: () -> number,
        newBody: (world: PhysicsWorld, x: number?, y: number?, kind: BodyType?) -> Body,
        newChainShape: (loop: boolean, ...any) -> PhysicsShape,
        newCircleShape: (...number) -> PhysicsShape,
        newDistanceJoint: (a: Body, b: Body, x1: number, y1: number, x2: number, y2: number, collideConnected: boolean?) -> Joint,
        newEdgeShape: (x1: number, y1: number, x2: number, y2: number) -> PhysicsShape,
        newFixture: (body: Body, shape: PhysicsShape, density: number?) -> Fixture,
        newMouseJoint: (body: Body, x: number, y: number) -> Joint,
        newPolygonShape: (...any) -> PhysicsShape,
        newPrismaticJoint: (a: Body, b: Body, ...any) -> Joint,
        newRectangleShape: (...number) -> PhysicsShape,
        newRevoluteJoint: (a: Body, b: Body, ...any) -> Joint,
        newRopeJoint: (a: Body, b: Body, x1: number, y1: number, x2: number, y2: number, maxLength: number, collideConnected: boolean?) -> Joint,
        newWeldJoint: (a: Body, b: Body, ...any) -> Joint,
        newWheelJoint: (a: Body, b: Body, ...any) -> Joint,
        newWorld: (gx: number?, gy: number?, sleep: boolean?) -> PhysicsWorld,
        setMeter: (meter: number) -> (),
    },
    timer: {
        getAverageDelta: () -> number,
        getDelta: () -> number,
        getFPS: () -> number,
        getTime: () -> number,
    },
    math: {
        colorFromBytes: (r: number, g: number, b: number, a: number?) -> (number, number, number, number),
        colorToBytes: (r: number, g: number, b: number, a: number?) -> (number, number, number, number),
        isConvex: (...any) -> boolean,
        newRandomGenerator: (seed: number?) -> RandomGenerator,
        noise: (x: number, y: number?, z: number?, w: number?) -> number,
        random: (min: number?, max: number?) -> number,
        randomNormal: (stddev: number?, mean: number?) -> number,
        setRandomSeed: (seed: number) -> (),
        triangulate: (...any) -> { { number } },
    },
    keyboard: {
        isDown: (...string) -> boolean,
        isScancodeDown: (...string) -> boolean,
    },
    mouse: {
        getCursor: () -> Cursor?,
        getPosition: () -> (number, number),
        getSystemCursor: (kind: string) -> Cursor,
        getX: () -> number,
        getY: () -> number,
        isCursorSupported: () -> boolean,
        isDown: (...number) -> boolean,
        isVisible: () -> boolean,
        setCursor: (cursor: Cursor?) -> (),
        setVisible: (visible: boolean) -> (),
    },
    touch: {
        getPosition: (id: number) -> (number, number),
        getTouches: () -> { number },
    },
    joystick: {
        getJoystickCount: () -> number,
        getJoysticks: () -> { Joystick },
    },
    audio: {
        getActiveSourceCount: () -> number,
        getPosition: () -> (number, number, number),
        getVolume: () -> number,
        newSource: (name: string, kind: ("static" | "stream")?) -> Source,
        pause: (...Source) -> (),
        play: (...Source) -> (),
        setPosition: (x: number, y: number, z: number?) -> (),
        setVolume: (volume: number) -> (),
        stop: (...Source) -> (),
    },
    window: {
        getDPIScale: () -> number,
        getDimensions: () -> (number, number),
        getMode: () -> (number, number, { [string]: any }),
        getSafeArea: () -> (number, number, number, number),
        setMode: (w: number, h: number, flags: { [string]: any }?) -> boolean,
    },
    system: {
        getOS: () -> string,
        vibrate: (seconds: number?) -> (),
    },
}

-- flux (rxi): a tween, from flux.to.
declare extern type FluxTween with
    function ease(self, name: string): FluxTween
    function delay(self, secs: number): FluxTween
    function onstart(self, fn: () -> ()): FluxTween
    function onupdate(self, fn: () -> ()): FluxTween
    function oncomplete(self, fn: () -> ()): FluxTween
    function after(self, ...: any): FluxTween
    function stop(self): ()
    function yoyo(self): FluxTween
end

declare flux: {
    to: (obj: any, secs: number, vars: { [string]: any }) -> FluxTween,
    update: (dt: number) -> (),
}

-- hump.timer: a timer, from timer.after, every or during.
declare extern type TimerHandle with
    function cancel(self): ()
end

declare extern type HumpTimer with
    after: (delay: number, fn: (...any) -> ...any) -> TimerHandle
    every: (delay: number, fn: (...any) -> ...any, count: number?) -> TimerHandle
    during: (delay: number, fn: (dt: number) -> ...any, after: ((...any) -> ...any)?) -> TimerHandle
    cancel: (handle: TimerHandle) -> ()
    clear: () -> ()
    script: (fn: (wait: (secs: number) -> ()) -> ()) -> ()
    tween: (secs: number, subject: any, target: { [string]: any }, method: string?, after: ((...any) -> ...any)?, ...any) -> FluxTween
    update: (dt: number) -> ()
    -- A 3D game's: engine 1.0's timer(name, secs, repeats), for on("timer", name).
    function __call(self, name: string, secs: number, repeats: boolean?): ()
end

declare timer: HumpTimer

-- anim8 (kikito): a grid makes frames (grid("1-8", 1), or grid:getFrames),
-- an animation plays them.
declare extern type Grid with
    function getFrames(self, ...: any): { Quad }
end

declare extern type Animation with
    position: number
    timer: number
    status: string
    flippedH: boolean
    flippedV: boolean
    totalDuration: number
    frames: { Quad }
    durations: { number }
    function update(self, dt: number): ()
    function draw(self, image: Image, x: number?, y: number?, r: number?, sx: number?, sy: number?, ox: number?, oy: number?): ()
    function gotoFrame(self, frame: number): ()
    function pause(self): ()
    function resume(self): ()
    function pauseAtEnd(self): ()
    function pauseAtStart(self): ()
    function flipH(self): Animation
    function flipV(self): Animation
    function clone(self): Animation
    function getDimensions(self): (number, number)
end

declare anim8: {
    newGrid: (frameWidth: number, frameHeight: number, imageWidth: number, imageHeight: number, left: number?, top: number?, border: number?) -> any,
    newAnimation: (frames: { Quad }, durations: number | { [any]: number }, onLoop: (string | (...any) -> ...any)?) -> Animation,
}

-- bump (kikito): a world of axis-aligned boxes.
declare extern type World with
    function add(self, item: any, x: number, y: number, w: number, h: number): any
    function remove(self, item: any): ()
    function update(self, item: any, x: number, y: number, w: number?, h: number?): ()
    function move(self, item: any, goalX: number, goalY: number, filter: ((item: any, other: any) -> string?)?): (number, number, { any }, number)
    function check(self, item: any, goalX: number, goalY: number, filter: ((item: any, other: any) -> string?)?): (number, number, { any }, number)
    function queryRect(self, x: number, y: number, w: number, h: number, filter: ((item: any) -> boolean)?): ({ any }, number)
    function queryPoint(self, x: number, y: number, filter: ((item: any) -> boolean)?): ({ any }, number)
    function hasItem(self, item: any): boolean
    function getRect(self, item: any): (number, number, number, number)
    function countItems(self): number
    function getItems(self): ({ any }, number)
    function getCellSize(self): number
end

declare bump: {
    newWorld: (cellSize: number?) -> World,
}

-- classic (rxi): Object:extend() makes a class; calling it makes one.
declare Object: {
    extend: (self: any) -> any,
    implement: (self: any, ...any) -> (),
    is: (self: any, class: any) -> boolean,
    new: (self: any, ...any) -> (),
}

-- LuaJIT's name for bit32, which LÖVE code uses.
declare bit: any

-- Lua's os, as far as a round has it: the round's clock (a date in 2026
-- picked by the round's seed, then game time), and exit, which ends the round.
declare os: {
    clock: () -> number,
    difftime: (t2: number, t1: number?) -> number,
    exit: (...any) -> (),
    time: () -> number,
}

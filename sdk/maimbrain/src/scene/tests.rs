use super::*;

fn view() -> View {
    View { w: 360.0, h: 640.0 }
}

#[test]
fn nodes_are_made_found_and_destroyed_with_their_children() {
    let mut s = Scene::new(360.0, 640.0);
    let group = s.add(Kind::Group, None).unwrap();
    let a = s.add(Kind::Sprite, Some(group)).unwrap();
    let b = s.add(Kind::Shape, Some(a)).unwrap();
    s.node_mut(b).unwrap().name = "hat".into();
    s.node_mut(a).unwrap().tag = "cat".into();
    assert_eq!(s.find_under(group, "hat"), Some(b));
    assert_eq!(s.find("hat"), Some(b));
    assert_eq!(s.all("cat"), [a]);
    assert_eq!(s.len(), 3);
    s.destroy(group);
    assert_eq!(s.len(), 0);
    assert!(!s.alive(a) && !s.alive(b));
    // An old id never reaches the node that reuses its slot.
    let c = s.add(Kind::Sprite, None).unwrap();
    assert_ne!(c, a);
    assert!(s.get(a).is_none() && s.get(b).is_none());
}

#[test]
fn the_cap_is_an_error() {
    let mut s = Scene::new(360.0, 640.0);
    for _ in 0..MAX_NODES {
        s.add(Kind::Group, None).unwrap();
    }
    assert_eq!(s.add(Kind::Group, None), Err(Full));
}

#[test]
fn velocity_and_gravity_move_nodes_in_rust() {
    let mut s = Scene::new(360.0, 640.0);
    let n = s.add(Kind::Sprite, None).unwrap();
    {
        let n = s.node_mut(n).unwrap();
        (n.vx, n.vy, n.gravity) = (60.0, 0.0, 100.0);
    }
    for _ in 0..60 {
        s.step(1.0 / 60.0);
    }
    let n = s.get(n).unwrap();
    assert!((n.x - 60.0).abs() < 1e-3, "{}", n.x);
    assert!((n.vy - 100.0).abs() < 1e-3);
    assert!(n.y > 49.0 && n.y < 52.0, "{}", n.y);
}

/// `vr` turns a node, driven ones too (a behaviour moves only its place),
/// and a spun rotation stays in −π..π.
#[test]
fn spin_turns_nodes_in_rust() {
    use std::f32::consts::PI;
    let mut s = Scene::new(360.0, 640.0);
    let n = s.add(Kind::Sprite, None).unwrap();
    {
        let n = s.node_mut(n).unwrap();
        (n.vr, n.driven, n.vx) = (2.0, true, 50.0);
    }
    for _ in 0..30 {
        s.step(1.0 / 60.0);
    }
    let r = s.get(n).unwrap().rotation;
    assert!((r - 1.0).abs() < 1e-4 && s.get(n).unwrap().x == 0.0, "{r}");
    for _ in 0..90 {
        s.step(1.0 / 60.0);
    }
    let r = s.get(n).unwrap().rotation;
    assert!((r - (4.0 - 2.0 * PI)).abs() < 1e-4, "4 radians reads as {r}");
}

#[test]
fn transforms_go_through_parents_and_reparenting_keeps_the_place() {
    let mut s = Scene::new(360.0, 640.0);
    let arm = s.add(Kind::Group, None).unwrap();
    {
        let n = s.node_mut(arm).unwrap();
        (n.x, n.y, n.rotation, n.sx, n.sy) = (100.0, 50.0, std::f32::consts::FRAC_PI_2, 2.0, 2.0);
    }
    let hand = s.add(Kind::Sprite, Some(arm)).unwrap();
    s.node_mut(hand).unwrap().x = 10.0;
    let (x, y) = s.world_position(hand);
    assert!((x - 100.0).abs() < 1e-3 && (y - 70.0).abs() < 1e-3, "{x} {y}");
    assert!(s.set_parent(hand, None, true));
    let n = s.get(hand).unwrap();
    assert!((n.x - 100.0).abs() < 1e-3 && (n.y - 70.0).abs() < 1e-3);
    assert!((n.rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-4 && (n.sx - 2.0).abs() < 1e-4);
    // Not under itself.
    let child = s.add(Kind::Group, Some(hand)).unwrap();
    assert!(!s.set_parent(hand, Some(child), true));
}

/// Nodes at order 0 or less, then the immediate layer, then those above:
/// by effective order (a child's adds its parent's), then tree order.
#[test]
fn draw_order_is_by_order_then_tree() {
    let mut s = Scene::new(360.0, 640.0);
    let sky = s.add(Kind::Shape, None).unwrap();
    s.node_mut(sky).unwrap().order = -10.0;
    let a = s.add(Kind::Shape, None).unwrap();
    let a_child = s.add(Kind::Shape, Some(a)).unwrap();
    let hud = s.add(Kind::Group, None).unwrap();
    s.node_mut(hud).unwrap().order = 5.0;
    let score = s.add(Kind::Text, Some(hud)).unwrap();
    let b = s.add(Kind::Shape, None).unwrap();
    let hidden = s.add(Kind::Shape, None).unwrap();
    s.node_mut(hidden).unwrap().visible = false;
    let ids: Vec<NodeId> = s.draw_order().into_iter().map(|e| e.0).collect();
    assert_eq!(ids, [sky, a, a_child, b, hud, score]);
    let split = s.draw_order().partition_point(|e| e.1 <= 0.0);
    assert_eq!(split, 4);
    // Alpha multiplies down; fixed is inherited.
    s.node_mut(hud).unwrap().alpha = 0.5;
    s.node_mut(hud).unwrap().fixed = true;
    let e = s.draw_order().into_iter().find(|e| e.0 == score).unwrap();
    assert_eq!((e.2, e.3), (0.5, true));
}

#[test]
fn the_camera_and_culling() {
    let mut s = Scene::new(360.0, 640.0);
    let img = s.add_image(SceneImage::new(32.0, 32.0));
    let near = s.add(Kind::Sprite, None).unwrap();
    let far = s.add(Kind::Sprite, None).unwrap();
    let hud = s.add(Kind::Shape, None).unwrap();
    for (id, x) in [(near, 100.0), (far, 2000.0)] {
        let n = s.node_mut(id).unwrap();
        (n.image, n.x, n.y) = (Some(img), x, 300.0);
    }
    {
        let n = s.node_mut(hud).unwrap();
        (n.x, n.y, n.fixed) = (20.0, 20.0, true);
    }
    let seen = |s: &Scene| s.on_view(&view()).into_iter().map(|e| e.0).collect::<Vec<_>>();
    assert_eq!(seen(&s), [near, hud]);
    // The camera follows the far one: the near one leaves, the HUD stays.
    s.camera.x = 2000.0;
    assert_eq!(seen(&s), [far, hud]);
    let (x0, y0, x1, y1) = s.on_view(&view())[0].1;
    assert_eq!((x0, y0, x1, y1), (164.0, 284.0, 196.0, 316.0));
    // Zoom: twice the size about the view's centre.
    s.camera.scale = 2.0;
    let (x0, _, x1, _) = s.on_view(&view())[0].1;
    assert_eq!((x0, x1), (148.0, 212.0));
    let (wx, wy) = s.camera.to_world(360.0, 640.0, 180.0, 320.0);
    assert_eq!((wx, wy), (2000.0, 320.0));
}

#[test]
fn sheets_have_frames_from_one() {
    let mut im = SceneImage::new(64.0, 32.0);
    (im.frame_w, im.frame_h) = (16.0, 16.0);
    assert_eq!(im.frames(), 8);
    assert_eq!(im.frame_src(1), [0.0, 0.0, 16.0, 16.0]);
    assert_eq!(im.frame_src(6), [16.0, 16.0, 16.0, 16.0]);
    assert_eq!(im.frame_src(9), [0.0, 0.0, 16.0, 16.0], "wraps");
}

#[test]
fn the_immediate_layer_records_and_caps() {
    let mut d = DrawList::new();
    assert!(d.push(Cmd::Text { s: "SCORE".into(), font: Font::Sans, size: 16.0, x: 0.0, y: 0.0, color: !0, wrap: 0.0, align: Align::Left }));
    assert_eq!(d.texts().collect::<Vec<_>>(), ["SCORE"]);
    for _ in 1..draw::MAX_CMDS {
        assert!(d.push(Cmd::Push));
    }
    assert!(!d.push(Cmd::Pop), "full");
    // Replaying an unbalanced list draws without leaking pushes.
    let s = Scene::new(360.0, 640.0);
    d.replay(&s, &view());
}

#[test]
fn affine_round_trips() {
    let m = Affine::trs(3.0, 4.0, 0.7, 2.0, 0.5);
    let (x, y) = m.inverse().apply(m.apply(5.0, -2.0).0, m.apply(5.0, -2.0).1);
    assert!((x - 5.0).abs() < 1e-4 && (y + 2.0).abs() < 1e-4);
    let (tx, ty, r, sx, sy) = m.decompose();
    assert!((tx - 3.0).abs() < 1e-5 && (ty - 4.0).abs() < 1e-5 && (r - 0.7).abs() < 1e-5 && (sx - 2.0).abs() < 1e-5 && (sy - 0.5).abs() < 1e-5);
}

/// A layer shakes with the view (fx.shake), whatever its parallax, as the
/// world's nodes do; a fixed one doesn't. It moves by its parallax share of
/// the camera's travel.
#[test]
fn layers_shake_with_the_view() {
    let mut s = Scene::new(360.0, 640.0);
    let l = s.add(Kind::Layer, None).unwrap();
    s.node_mut(l).unwrap().parallax = 0.0;
    assert_eq!(s.layer_centre(s.get(l).unwrap(), false), (180.0, 320.0));
    (s.camera.shake_x, s.camera.shake_y) = (4.0, -3.0);
    assert_eq!(s.layer_centre(s.get(l).unwrap(), false), (184.0, 317.0), "a still layer shakes too");
    assert_eq!(s.layer_centre(s.get(l).unwrap(), true), (180.0, 320.0), "a fixed one doesn't");
    s.camera.x += 100.0;
    s.node_mut(l).unwrap().parallax = 0.5;
    assert_eq!(s.layer_centre(s.get(l).unwrap(), false), (234.0, 317.0));
}

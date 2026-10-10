//! A humanoid's bones by what they are, whatever the rig called them:
//! [`Humanoid::find`] reads a spawned model's bone names (Tripo's `L_Thigh`,
//! Mixamo's `mixamorig:LeftUpLeg`, Blender's `thigh.L`, Unity-style
//! `LeftUpperLeg`, 3ds Max's `Bip01 L Thigh`…) and adopts the ones it
//! recognises as a [`Skeleton`] with semantic names: `hips`, `spine`,
//! `chest`, `neck`, `head`, then `shoulder_l`, `upper_arm_l`, `forearm_l`,
//! `hand_l`, `thigh_l`, `shin_l`, `foot_l`, `toe_l` and the `_r` ones.
//! Pose them in the model's frame (`turn_model`), so a hand-keyed pose
//! doesn't depend on the rig's bone axes.
//!
//! ```ignore
//! let mech = model.spawn(Some(body))?;
//! let rig = Humanoid::find(model, mech)?;          // right after spawning
//! rig.turn("chest", Quat::from_rotation_y(-0.35)); // wind up: the torso turns away
//! rig.turn("forearm_r", Quat::from_rotation_x(-1.2));
//! let hand = rig.node("hand_r");                   // to attach a blade to
//! ```

use super::math::{Quat, Vec3};
use super::skin::Skeleton;
use super::{Model, Node};

/// The semantic bones, in the order a [`Humanoid`]'s skeleton lists the ones
/// it found (parents before children).
pub const HUMANOID: [&str; 21] = [
    "hips", "spine", "chest", "neck", "head", "shoulder_l", "upper_arm_l", "forearm_l", "hand_l", "shoulder_r", "upper_arm_r", "forearm_r", "hand_r", "thigh_l",
    "shin_l", "foot_l", "toe_l", "thigh_r", "shin_r", "foot_r", "toe_r",
];

/// What one bone name says: a body part (without a side) and the side, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Hips,
    /// A spine bone (several: the lowest becomes `spine`, the highest `chest`).
    Spine,
    Chest,
    Neck,
    Head,
    Shoulder,
    UpperArm,
    Forearm,
    Hand,
    Thigh,
    Shin,
    Foot,
    Toe,
}

/// Reads a bone name: `Some((part, side))` with side `'l'`, `'r'` or `' '`.
/// Twist, roll, end, IK and finger bones are None.
pub fn classify(name: &str) -> Option<(Part, char)> {
    let mut s = name.to_ascii_lowercase();
    for sep in [':', '|'] {
        if let Some(i) = s.rfind(sep) {
            s = s[i + 1..].to_string();
        }
    }
    for p in ["def-", "org-", "def_", "bip01 ", "bip001 ", "bip01_", "bip001_", "b_", "bn_", "j_", "jnt_"] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest.to_string();
        }
    }
    if ["end", "nub", "_ik", "ik_", "pole", "target", "ctrl", "finger", "thumb", "index", "middle", "ring", "pinky", "eye", "jaw", "tongue", "hair", "_tip"].iter().any(|w| s.contains(w))
        || s.ends_with("ik")
    {
        return None;
    }
    // The side: a prefix (`l_`, `left`), a suffix (`.l`, `_r`, ` left`) or a word inside (`leftupleg`).
    let mut side = ' ';
    let pre = [("left", 'l'), ("right", 'r'), ("l_", 'l'), ("r_", 'r'), ("l.", 'l'), ("r.", 'r'), ("l ", 'l'), ("r ", 'r'), ("l-", 'l'), ("r-", 'r')];
    let suf = [("left", 'l'), ("right", 'r'), (".l", 'l'), (".r", 'r'), ("_l", 'l'), ("_r", 'r'), (" l", 'l'), (" r", 'r'), ("-l", 'l'), ("-r", 'r')];
    if let Some((p, c)) = pre.iter().find(|(p, _)| s.starts_with(p)) {
        side = *c;
        s = s[p.len()..].to_string();
    } else if let Some((p, c)) = suf.iter().find(|(p, _)| s.ends_with(p)) {
        side = *c;
        s = s[..s.len() - p.len()].to_string();
    } else if let Some(i) = s.find("left") {
        side = 'l';
        s.replace_range(i..i + 4, "");
    } else if let Some(i) = s.find("right") {
        side = 'r';
        s.replace_range(i..i + 5, "");
    }
    // Blender's `spine.003`, Tripo's `Spine01`: keep the word, drop separators and numbers.
    let word: String = s.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let limb = |p: Part| if word.contains("twist") || word.contains("roll") || word.contains("share") { None } else { Some((p, side)) };
    let part = match word.as_str() {
        "hips" | "hip" | "pelvis" => (Part::Hips, side),
        "spine" | "waist" | "torso" | "abdomen" | "belly" | "lowerspine" | "midspine" => (Part::Spine, side),
        "chest" | "upperchest" | "upperspine" | "ribcage" => (Part::Chest, side),
        "neck" | "necktwist" | "upperneck" => (Part::Neck, side),
        "head" => (Part::Head, side),
        "clavicle" | "shoulder" | "collar" | "collarbone" => (Part::Shoulder, side),
        "upperarm" | "arm" | "uparm" | "humerus" => (Part::UpperArm, side),
        "forearm" | "lowerarm" | "elbow" | "loarm" => (Part::Forearm, side),
        "hand" | "wrist" => (Part::Hand, side),
        "thigh" | "upleg" | "upperleg" | "femur" => (Part::Thigh, side),
        "calf" | "shin" | "leg" | "lowerleg" | "knee" | "loleg" => (Part::Shin, side),
        "foot" | "ankle" => (Part::Foot, side),
        "toe" | "toes" | "toebase" | "ball" => (Part::Toe, side),
        w if w.contains("twist") || w.contains("roll") => return None,
        _ => return None,
    };
    if matches!(part.0, Part::Hips | Part::Spine | Part::Chest | Part::Neck | Part::Head) {
        return Some((part.0, ' '));
    }
    if side == ' ' {
        return None;
    }
    limb(part.0)
}

/// Maps a rig's bones (each name with its parent's name) to the semantic
/// names in [`HUMANOID`]: `(semantic, rig name)` in that order, for the ones
/// found. Where several bones say the same thing (Mixamo's `Spine`,
/// `Spine1`, `Spine2`; Tripo's `Hip` and `Pelvis`), the one nearest the root
/// is `hips` and `spine`, the farthest `chest`.
pub fn humanoid_map(bones: &[(String, Option<String>)]) -> Vec<(&'static str, String)> {
    let depth = |name: &str| -> usize {
        let mut d = 0;
        let mut at = name;
        while let Some((_, Some(p))) = bones.iter().find(|b| b.0 == at) {
            d += 1;
            at = p;
            if d > 128 {
                break;
            }
        }
        d
    };
    // Blender's metarig numbers its spine: spine (hips) … spine.003 (chest), .004–.005 neck, .006 head.
    let metarig = bones.iter().any(|b| b.0.eq_ignore_ascii_case("spine.006") || b.0.eq_ignore_ascii_case("def-spine.006"));
    let mut found: Vec<(Part, char, &str, usize)> = Vec::new();
    for (name, _) in bones {
        let c = if metarig && name.to_ascii_lowercase().trim_start_matches("def-").starts_with("spine") {
            let n: String = name.chars().filter(|c| c.is_ascii_digit()).collect();
            match n.parse::<u32>().unwrap_or(0) {
                0 => Some((Part::Hips, ' ')),
                1 | 2 => Some((Part::Spine, ' ')),
                3 => Some((Part::Chest, ' ')),
                4 | 5 => Some((Part::Neck, ' ')),
                _ => Some((Part::Head, ' ')),
            }
        } else {
            classify(name)
        };
        if let Some((p, s)) = c {
            found.push((p, s, name.as_str(), depth(name)));
        }
    }
    let pick = |p: Part, side: char, deepest: bool| -> Option<&str> {
        let it = found.iter().filter(|f| f.0 == p && f.1 == side);
        if deepest { it.max_by_key(|f| f.3).map(|f| f.2) } else { it.min_by_key(|f| f.3).map(|f| f.2) }
    };
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let hips = pick(Part::Hips, ' ', false);
    // Spine bones: the explicit ones, plus Hips-like bones under the hips (Tripo's Pelvis is a sibling, so it isn't).
    let spine = pick(Part::Spine, ' ', false);
    let chest = pick(Part::Chest, ' ', true).or_else(|| pick(Part::Spine, ' ', true).filter(|c| Some(*c) != spine));
    let neck = pick(Part::Neck, ' ', false);
    let head = pick(Part::Head, ' ', false);
    for (sem, name) in [("hips", hips), ("spine", spine), ("chest", chest), ("neck", neck), ("head", head)] {
        if let Some(n) = name {
            out.push((sem, n.to_string()));
        }
    }
    for (side, sfx) in [('l', "l"), ('r', "r")] {
        for (p, sem) in [(Part::Shoulder, "shoulder"), (Part::UpperArm, "upper_arm"), (Part::Forearm, "forearm"), (Part::Hand, "hand")] {
            if let Some(n) = pick(p, side, false) {
                out.push((HUMANOID.iter().copied().find(|h| *h == format!("{sem}_{sfx}")).unwrap(), n.to_string()));
            }
        }
    }
    for (side, sfx) in [('l', "l"), ('r', "r")] {
        for (p, sem) in [(Part::Thigh, "thigh"), (Part::Shin, "shin"), (Part::Foot, "foot"), (Part::Toe, "toe")] {
            if let Some(n) = pick(p, side, false) {
                out.push((HUMANOID.iter().copied().find(|h| *h == format!("{sem}_{sfx}")).unwrap(), n.to_string()));
            }
        }
    }
    out
}

/// A humanoid (or a biped mech) rig's bones by semantic name, adopted as a
/// [`Skeleton`] (its bones named `hips`, `chest`, `thigh_l`…, each under its
/// real parent), with the rig's own name for each.
#[derive(Clone, Debug, PartialEq)]
pub struct Humanoid {
    pub skeleton: Skeleton,
    /// The rig's name for each bone, by index.
    pub rig_names: Vec<String>,
}

impl Humanoid {
    /// Recognises a spawned model's humanoid bones (call right after
    /// spawning: rest poses are read from where the bones are). None if it
    /// has no hips or no legs. Log `rig_names` once to see what it found.
    pub fn find(model: Model, root: Node) -> Option<Humanoid> {
        let names = model.node_names();
        let nodes: Vec<(String, Node)> = names.iter().filter_map(|n| Some((n.clone(), root.find(n)?))).collect();
        let pairs: Vec<(String, Option<String>)> =
            nodes.iter().map(|(n, node)| (n.clone(), node.parent().and_then(|p| nodes.iter().find(|x| x.1 == p).map(|x| x.0.clone())))).collect();
        let map = humanoid_map(&pairs);
        if !map.iter().any(|m| m.0 == "hips") || !map.iter().any(|m| m.0 == "thigh_l") {
            return None;
        }
        // Parents before children: adopt in depth order.
        let mut list: Vec<(Node, &str, &str)> = map.iter().filter_map(|(sem, rig)| Some((nodes.iter().find(|x| &x.0 == rig)?.1, *sem, rig.as_str()))).collect();
        let depth = |n: Node| {
            let mut d = 0;
            let mut p = n.parent();
            while let Some(x) = p {
                d += 1;
                p = x.parent();
                if d > 128 {
                    break;
                }
            }
            d
        };
        list.sort_by_key(|x| depth(x.0));
        let adopt: Vec<(Node, &str)> = list.iter().map(|x| (x.0, x.1)).collect();
        let skeleton = Skeleton::adopt_nodes(root, &adopt)?;
        Some(Humanoid { skeleton, rig_names: list.iter().map(|x| x.2.to_string()).collect() })
    }

    /// The index of semantic bone `name` (`"hips"`, `"hand_r"`…) in the skeleton.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.skeleton.find(name)
    }

    /// Semantic bone `name`'s node (to parent a part to, or read its `world()`).
    pub fn node(&self, name: &str) -> Option<Node> {
        self.skeleton.bone(self.index(name)?)
    }

    /// Turns semantic bone `name` by `rot` in the model's frame, relative to
    /// its parent ([`Skeleton::turn_model`]). Ignored if the rig hasn't it.
    pub fn turn(&self, name: &str, rot: Quat) {
        if let Some(i) = self.index(name) {
            self.skeleton.turn_model(i, rot);
        }
    }

    /// Where semantic bone `name` sits at rest, in the model's frame.
    pub fn rest_point(&self, name: &str) -> Option<Vec3> {
        self.skeleton.rest_point(self.index(name)?)
    }

    /// Which side of the model (−1 for −x, +1 for +x) the rig's `_l` bones
    /// are on: generated rigs differ, so a pose written for "the sword arm on
    /// −x" can check and mirror.
    pub fn left_x(&self) -> f32 {
        match (self.rest_point("hand_l").or(self.rest_point("thigh_l")), self.rest_point("hand_r").or(self.rest_point("thigh_r"))) {
            (Some(l), Some(r)) if l.x > r.x => 1.0,
            _ => -1.0,
        }
    }

    /// Puts every bone back in its rest pose.
    pub fn reset(&self) {
        self.skeleton.reset();
    }

    /// Hangs `part` on semantic bone `bone` where it is now (see
    /// [`Node::attach_to`]): place a shield on the arm at rest, attach, and
    /// it follows the arm. False if the rig hasn't the bone.
    pub fn attach(&self, part: Node, bone: &str) -> bool {
        self.node(bone).is_some_and(|b| part.attach_to(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(list: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        list.iter().map(|(a, b)| (a.to_string(), b.map(str::to_string))).collect()
    }

    fn get<'a>(m: &'a [(&'static str, String)], sem: &str) -> Option<&'a str> {
        m.iter().find(|x| x.0 == sem).map(|x| x.1.as_str())
    }

    #[test]
    fn tripo_biped_names() {
        // The 41-joint skeleton Tripo's biped rig gives (twist bones included).
        let t = tree(&[
            ("Root", None),
            ("Hip", Some("Root")),
            ("Pelvis", Some("Hip")),
            ("L_Thigh", Some("Pelvis")),
            ("L_ThighTwist01", Some("L_Thigh")),
            ("L_Calf", Some("L_Thigh")),
            ("L_CalfTwist01", Some("L_Calf")),
            ("L_Foot", Some("L_Calf")),
            ("L_ToeBase", Some("L_Foot")),
            ("R_Thigh", Some("Pelvis")),
            ("R_Calf", Some("R_Thigh")),
            ("R_Foot", Some("R_Calf")),
            ("R_ToeBase", Some("R_Foot")),
            ("Waist", Some("Hip")),
            ("Spine01", Some("Waist")),
            ("Spine02", Some("Spine01")),
            ("NeckTwist01", Some("Spine02")),
            ("NeckTwist02", Some("NeckTwist01")),
            ("Head", Some("NeckTwist02")),
            ("L_Clavicle", Some("Spine02")),
            ("L_Upperarm", Some("L_Clavicle")),
            ("L_UpperarmTwist01", Some("L_Upperarm")),
            ("L_Forearm", Some("L_Upperarm")),
            ("L_ForearmTwist01", Some("L_Forearm")),
            ("L_Hand", Some("L_Forearm")),
            ("R_Clavicle", Some("Spine02")),
            ("R_Upperarm", Some("R_Clavicle")),
            ("R_Forearm", Some("R_Upperarm")),
            ("R_Hand", Some("R_Forearm")),
        ]);
        let m = humanoid_map(&t);
        let want = [
            ("hips", "Hip"),
            ("spine", "Waist"),
            ("chest", "Spine02"),
            ("neck", "NeckTwist01"),
            ("head", "Head"),
            ("shoulder_l", "L_Clavicle"),
            ("upper_arm_l", "L_Upperarm"),
            ("forearm_l", "L_Forearm"),
            ("hand_l", "L_Hand"),
            ("hand_r", "R_Hand"),
            ("thigh_l", "L_Thigh"),
            ("shin_l", "L_Calf"),
            ("foot_l", "L_Foot"),
            ("toe_l", "L_ToeBase"),
            ("shin_r", "R_Calf"),
        ];
        for (sem, rig) in want {
            assert_eq!(get(&m, sem), Some(rig), "{sem}: {m:?}");
        }
        assert!(!m.iter().any(|x| x.1.contains("Twist0") && x.0 != "neck"), "no limb twist bones: {m:?}");
    }

    #[test]
    fn mixamo_names() {
        let t = tree(&[
            ("mixamorig:Hips", None),
            ("mixamorig:Spine", Some("mixamorig:Hips")),
            ("mixamorig:Spine1", Some("mixamorig:Spine")),
            ("mixamorig:Spine2", Some("mixamorig:Spine1")),
            ("mixamorig:Neck", Some("mixamorig:Spine2")),
            ("mixamorig:Head", Some("mixamorig:Neck")),
            ("mixamorig:HeadTop_End", Some("mixamorig:Head")),
            ("mixamorig:LeftShoulder", Some("mixamorig:Spine2")),
            ("mixamorig:LeftArm", Some("mixamorig:LeftShoulder")),
            ("mixamorig:LeftForeArm", Some("mixamorig:LeftArm")),
            ("mixamorig:LeftHand", Some("mixamorig:LeftForeArm")),
            ("mixamorig:LeftHandIndex1", Some("mixamorig:LeftHand")),
            ("mixamorig:RightUpLeg", Some("mixamorig:Hips")),
            ("mixamorig:RightLeg", Some("mixamorig:RightUpLeg")),
            ("mixamorig:RightFoot", Some("mixamorig:RightLeg")),
            ("mixamorig:RightToeBase", Some("mixamorig:RightFoot")),
            ("mixamorig:LeftUpLeg", Some("mixamorig:Hips")),
        ]);
        let m = humanoid_map(&t);
        for (sem, rig) in [
            ("hips", "mixamorig:Hips"),
            ("spine", "mixamorig:Spine"),
            ("chest", "mixamorig:Spine2"),
            ("neck", "mixamorig:Neck"),
            ("head", "mixamorig:Head"),
            ("shoulder_l", "mixamorig:LeftShoulder"),
            ("upper_arm_l", "mixamorig:LeftArm"),
            ("forearm_l", "mixamorig:LeftForeArm"),
            ("hand_l", "mixamorig:LeftHand"),
            ("thigh_r", "mixamorig:RightUpLeg"),
            ("shin_r", "mixamorig:RightLeg"),
            ("foot_r", "mixamorig:RightFoot"),
            ("toe_r", "mixamorig:RightToeBase"),
            ("thigh_l", "mixamorig:LeftUpLeg"),
        ] {
            assert_eq!(get(&m, sem), Some(rig), "{sem}: {m:?}");
        }
    }

    #[test]
    fn blender_and_other_names() {
        let meta = tree(&[
            ("spine", None),
            ("spine.001", Some("spine")),
            ("spine.002", Some("spine.001")),
            ("spine.003", Some("spine.002")),
            ("spine.004", Some("spine.003")),
            ("spine.005", Some("spine.004")),
            ("spine.006", Some("spine.005")),
            ("shoulder.L", Some("spine.003")),
            ("upper_arm.L", Some("shoulder.L")),
            ("forearm.L", Some("upper_arm.L")),
            ("hand.L", Some("forearm.L")),
            ("thigh.R", Some("spine")),
            ("shin.R", Some("thigh.R")),
            ("foot.R", Some("shin.R")),
            ("toe.R", Some("foot.R")),
        ]);
        let m = humanoid_map(&meta);
        for (sem, rig) in [
            ("hips", "spine"),
            ("spine", "spine.001"),
            ("chest", "spine.003"),
            ("neck", "spine.004"),
            ("head", "spine.006"),
            ("upper_arm_l", "upper_arm.L"),
            ("hand_l", "hand.L"),
            ("thigh_r", "thigh.R"),
            ("shin_r", "shin.R"),
            ("toe_r", "toe.R"),
        ] {
            assert_eq!(get(&m, sem), Some(rig), "{sem}: {m:?}");
        }
        // Unity-style, 3ds Max Biped and the koi's own skeleton.
        assert_eq!(classify("LeftUpperLeg"), Some((Part::Thigh, 'l')));
        assert_eq!(classify("RightLowerArm"), Some((Part::Forearm, 'r')));
        assert_eq!(classify("Bip01 L Thigh"), Some((Part::Thigh, 'l')));
        assert_eq!(classify("Bip01 R Calf"), Some((Part::Shin, 'r')));
        assert_eq!(classify("Hand.R"), Some((Part::Hand, 'r')));
        assert_eq!(classify("UpperChest"), Some((Part::Chest, ' ')));
        assert_eq!(classify("L_UpperarmTwist01"), None);
        assert_eq!(classify("spine_3"), Some((Part::Spine, ' ')));
        assert_eq!(classify("wing_l_0"), None);
        assert_eq!(classify("Socket.Tail"), None);
        assert_eq!(classify("Thigh"), None, "a limb needs a side");
    }

    #[test]
    fn find_is_none_off_wasm() {
        // Off wasm a model has no node names, so nothing is recognised (and nothing panics).
        let root = Node::new().unwrap();
        let m = Model(std::num::NonZeroU32::new(1).unwrap());
        assert!(Humanoid::find(m, root).is_none());
    }
}

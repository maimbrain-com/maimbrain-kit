//! Meshy (3D models, the second provider for `mb model`):
//! https://api.meshy.ai/openapi. Each step is an async task: POST returns
//! `{"result": id}`, `GET <path>/<id>` reports `status` (PENDING,
//! IN_PROGRESS, SUCCEEDED, FAILED, CANCELED) and `progress`; results are
//! signed URLs kept 3 days.
//!
//!   text   POST /v2/text-to-3d {mode: "preview", …} then {mode: "refine", preview_task_id, enable_pbr}
//!   image  POST /v1/image-to-3d {image_url: data URI, should_texture, enable_pbr, …}
//!   rig    POST /v1/rigging {input_task_id, height_meters}: humanoids only; walk and run clips come with it
//!   clips  POST /v1/animations {rig_task_id, action_id}; ids from GET /v1/animations/library?search=
//!
//! Credits: preview 20 (meshy-7.1) or 5 (6-lite), refine 10, rig 5,
//! animation 3; at the Pro plan's $20 for 1000, $0.02 each. Failed tasks are
//! refunded. Docs: https://docs.meshy.ai/api

use serde_json::{Value, json};

use super::provider::{AudioRequest, ImageRequest, ModelOutput, ModelRequest, Output, Poll, Provider, Transport, b64, download, http_error};

const BASE: &str = "https://api.meshy.ai/openapi";
/// USD per credit on the Pro plan ($20 for 1000).
const CREDIT: f64 = 0.02;

pub struct Meshy {
    key: String,
    http: Box<dyn Transport>,
    poll: Poll,
}

impl Meshy {
    pub fn new(key: String, http: Box<dyn Transport>, poll: Poll) -> Meshy {
        Meshy { key, http, poll }
    }

    fn auth(&self) -> Vec<(String, String)> {
        vec![("Authorization".to_string(), format!("Bearer {}", self.key))]
    }

    fn create(&self, path: &str, body: &Value) -> Result<String, String> {
        let r = self.http.post(&format!("{BASE}{path}"), &self.auth(), "application/json", serde_json::to_vec(body).unwrap())?;
        if !(200..300).contains(&r.status) {
            return Err(http_error("meshy", &r));
        }
        let v: Value = serde_json::from_slice(&r.body).map_err(|e| format!("meshy: bad response: {e}"))?;
        v["result"].as_str().map(str::to_string).ok_or_else(|| "meshy: no task id in the response".into())
    }

    fn wait(&self, what: &str, path: &str, id: &str, credits: &mut f64) -> Result<Value, String> {
        let v = self.poll.wait(&format!("meshy {what}"), || {
            let r = self.http.get(&format!("{BASE}{path}/{id}"), &self.auth())?;
            if r.status != 200 {
                return Err(http_error("meshy", &r));
            }
            let v: Value = serde_json::from_slice(&r.body).map_err(|e| format!("meshy: bad response: {e}"))?;
            match v["status"].as_str().unwrap_or("") {
                "PENDING" => Ok((None, "queued".into())),
                "IN_PROGRESS" => Ok((None, format!("in progress {}%", v["progress"].as_u64().unwrap_or(0)))),
                "SUCCEEDED" => Ok((Some(v), "done".into())),
                s => Err(format!(
                    "meshy {what} {}: {} (task {id})",
                    s.to_lowercase(),
                    v["task_error"]["message"].as_str().filter(|m| !m.is_empty()).unwrap_or("no reason given")
                )),
            }
        })?;
        *credits += v["consumed_credits"].as_f64().unwrap_or(0.0);
        Ok(v)
    }

    fn fetch(&self, what: &str, url: Option<&str>) -> Result<Vec<u8>, String> {
        let url = url.filter(|u| !u.is_empty()).ok_or(format!("meshy {what}: the result has no .glb"))?;
        let b = download(self.http.as_ref(), "meshy", url)?;
        if !b.starts_with(b"glTF") {
            return Err(format!("meshy {what}: the download isn't a .glb"));
        }
        Ok(b)
    }

    /// An action id from the animation library: a number as is, else the best name match.
    fn action_id(&self, action: &str) -> Result<i64, String> {
        if let Ok(n) = action.parse::<i64>() {
            return Ok(n);
        }
        let r = self.http.get(&format!("{BASE}/v1/animations/library?search={}", action.replace(' ', "%20")), &self.auth())?;
        if r.status != 200 {
            return Err(http_error("meshy", &r));
        }
        let list: Vec<Value> = serde_json::from_slice(&r.body).unwrap_or_default();
        let want = action.to_lowercase();
        let lower = |a: &Value, f: &str| a[f].as_str().unwrap_or("").to_lowercase();
        let exact = list.iter().find(|a| lower(a, "name") == want || lower(a, "key") == want);
        let close = list.iter().find(|a| lower(a, "key").starts_with(&format!("{want}_")) || lower(a, "name").ends_with(&format!(" {want}")));
        exact
            .or(close)
            .or(list.first())
            .and_then(|a| a["action_id"].as_i64())
            .ok_or(format!("Meshy's animation library has nothing called {action:?} (browse https://docs.meshy.ai/api/animation-library and pass the action id)"))
    }
}

impl Provider for Meshy {
    fn name(&self) -> &'static str {
        "meshy"
    }

    fn image(&self, _req: &ImageRequest) -> Result<Output, String> {
        Err("meshy makes 3D models (mb model), not images".into())
    }

    fn audio(&self, _req: &AudioRequest) -> Result<Output, String> {
        Err("meshy makes 3D models (mb model), not sound".into())
    }

    fn model(&self, req: &ModelRequest) -> Result<ModelOutput, String> {
        let mut credits = 0.0;
        let mut out = ModelOutput::default();
        let rigging = req.rig || !req.animations.is_empty();
        let polys = req.polys.clamp(100, 300_000);
        let pose = if rigging { "a-pose" } else { "" };
        let prompt: String = req.prompt.chars().take(800).collect();
        let source = match &req.image {
            Some(img) => {
                let body = json!({
                    "image_url": format!("data:{};base64,{}", img.mime, b64(&img.bytes)),
                    "ai_model": req.model, "should_remesh": true, "topology": "triangle", "target_polycount": polys,
                    "should_texture": true, "enable_pbr": true, "texture_prompt": prompt, "pose_mode": pose, "target_formats": ["glb"],
                });
                let id = self.create("/v1/image-to-3d", &body)?;
                out.tasks.push(("image-to-3d".into(), id.clone()));
                let v = self.wait("image-to-3d", "/v1/image-to-3d", &id, &mut credits)?;
                out.glb = self.fetch("image-to-3d", v["model_urls"]["glb"].as_str())?;
                id
            }
            None => {
                let body = json!({
                    "mode": "preview", "prompt": prompt, "ai_model": req.model, "should_remesh": true, "topology": "triangle",
                    "target_polycount": polys, "pose_mode": pose, "target_formats": ["glb"],
                });
                let pid = self.create("/v2/text-to-3d", &body)?;
                out.tasks.push(("preview".into(), pid.clone()));
                self.wait("preview (shape)", "/v2/text-to-3d", &pid, &mut credits)?;
                let body = json!({ "mode": "refine", "preview_task_id": pid, "enable_pbr": true, "texture_prompt": prompt, "target_formats": ["glb"] });
                let rid = self.create("/v2/text-to-3d", &body)?;
                out.tasks.push(("refine".into(), rid.clone()));
                let v = self.wait("refine (texture)", "/v2/text-to-3d", &rid, &mut credits)?;
                out.glb = self.fetch("refine", v["model_urls"]["glb"].as_str())?;
                rid
            }
        };
        out.cost_usd = Some(credits * CREDIT);
        if !rigging {
            return Ok(out);
        }
        let rig = self.create("/v1/rigging", &json!({ "input_task_id": source, "height_meters": 1.7 })).and_then(|id| {
            out.tasks.push(("rig".into(), id.clone()));
            let v = self.wait("rig", "/v1/rigging", &id, &mut credits)?;
            let glb = self.fetch("rig", v["result"]["rigged_character_glb_url"].as_str())?;
            Ok((id, glb, v))
        });
        let (rig_id, rig_v) = match rig {
            Ok((id, glb, v)) => {
                out.glb = glb;
                out.rigged = true;
                (id, v)
            }
            Err(e) => {
                out.notes.push(format!("{e}; Meshy rigs humanoids only, so kept the model unrigged (tripo rigs animals and fish)"));
                out.cost_usd = Some(credits * CREDIT);
                return Ok(out);
            }
        };
        for a in &req.animations {
            let basic = &rig_v["result"]["basic_animations"];
            let free = match a.to_lowercase().as_str() {
                "walk" | "walking" => basic["walking_glb_url"].as_str(),
                "run" | "running" => basic["running_glb_url"].as_str(),
                _ => None,
            };
            let got = match free {
                Some(url) => self.fetch(a, Some(url)),
                None => self.action_id(a).and_then(|action| {
                    let id = self.create("/v1/animations", &json!({ "rig_task_id": rig_id, "action_id": action }))?;
                    out.tasks.push((format!("animate {a} (action {action})"), id.clone()));
                    let v = self.wait(&format!("animate {a}"), "/v1/animations", &id, &mut credits)?;
                    self.fetch(a, v["result"]["animation_glb_url"].as_str())
                }),
            };
            match got {
                Ok(b) => out.clips.push((a.clone(), b)),
                Err(e) => out.notes.push(format!("{e}; no {a} clip")),
            }
        }
        out.cost_usd = Some(credits * CREDIT);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider::tests::{Fake, json as reply};
    use super::super::provider::Response;
    use super::*;
    use std::rc::Rc;
    use std::time::Duration;

    fn meshy(fake: &Rc<Fake>) -> Meshy {
        Meshy::new("msy_test".into(), Box::new(fake.clone()), Poll { every: Duration::ZERO, timeout: Duration::from_secs(5) })
    }

    fn file() -> Response {
        Response { status: 200, content_type: "model/gltf-binary".into(), body: b"glTF\x02\0\0\0".to_vec() }
    }

    #[test]
    fn a_running_character_preview_refine_rig_and_clips() {
        let fake = Rc::new(Fake::new(vec![
            reply(202, json!({ "result": "pre1" })),
            reply(200, json!({ "id": "pre1", "status": "IN_PROGRESS", "progress": 42, "task_error": { "message": "" }, "consumed_credits": 20 })),
            reply(200, json!({ "id": "pre1", "status": "SUCCEEDED", "progress": 100, "model_urls": { "glb": "https://assets.meshy.ai/t/pre.glb?Expires=1" }, "task_error": { "message": "" }, "consumed_credits": 20 })),
            reply(202, json!({ "result": "ref1" })),
            reply(200, json!({ "id": "ref1", "status": "SUCCEEDED", "progress": 100, "model_urls": { "glb": "https://assets.meshy.ai/t/model.glb?Expires=1" }, "consumed_credits": 10 })),
            file(),
            reply(202, json!({ "result": "rig1" })),
            reply(200, json!({ "id": "rig1", "type": "rig", "status": "SUCCEEDED", "consumed_credits": 5, "result": {
                "rigged_character_glb_url": "https://assets.meshy.ai/t/Character_output.glb",
                "basic_animations": { "walking_glb_url": "https://assets.meshy.ai/t/walk.glb", "running_glb_url": "https://assets.meshy.ai/t/run.glb" } } })),
            file(),
            file(), // run: free with the rig
            reply(200, json!([{ "action_id": 11, "name": "Idle_02", "key": "Idle_02" }, { "action_id": 0, "name": "Idle", "key": "Idle" }])),
            reply(202, json!({ "result": "anim1" })),
            reply(200, json!({ "id": "anim1", "type": "animate", "status": "SUCCEEDED", "consumed_credits": 3, "result": { "animation_glb_url": "https://assets.meshy.ai/t/idle.glb" } })),
            file(),
        ]));
        let req = ModelRequest { model: "meshy-7.1".into(), prompt: "a runner".into(), polys: 6000, animations: vec!["run".into(), "idle".into()], ..ModelRequest::default() };
        let out = meshy(&fake).model(&req).unwrap();
        assert!(out.rigged && out.notes.is_empty(), "{:?}", out.notes);
        assert_eq!(out.clips.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["run", "idle"]);
        assert!((out.cost_usd.unwrap() - 38.0 * 0.02).abs() < 1e-9);
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "https://api.meshy.ai/openapi/v2/text-to-3d");
        assert_eq!(sent[0].1[0].1, "Bearer msy_test");
        assert_eq!(sent[0].2["mode"], "preview");
        assert_eq!(sent[0].2["target_polycount"], 6000);
        assert_eq!(sent[0].2["pose_mode"], "a-pose");
        assert_eq!(sent[1].0, "GET https://api.meshy.ai/openapi/v2/text-to-3d/pre1");
        assert_eq!(sent[3].2["preview_task_id"], "pre1");
        assert_eq!(sent[3].2["enable_pbr"], true);
        assert_eq!(sent[6].2["input_task_id"], "ref1");
        assert!(sent[10].0.ends_with("/v1/animations/library?search=idle"));
        assert_eq!(sent[11].2["action_id"], 0, "the exact name wins over the first hit");
        assert_eq!(sent[11].2["rig_task_id"], "rig1");
    }

    #[test]
    fn a_fish_cant_be_rigged_so_it_comes_back_unrigged() {
        let fake = Rc::new(Fake::new(vec![
            reply(202, json!({ "result": "img1" })),
            reply(200, json!({ "id": "img1", "status": "SUCCEEDED", "model_urls": { "glb": "https://assets.meshy.ai/t/m.glb" }, "consumed_credits": 30 })),
            file(),
            reply(422, json!({ "message": "Pose estimation failed" })),
        ]));
        let req = ModelRequest {
            model: "meshy-7.1".into(),
            prompt: "a koi".into(),
            image: Some(super::super::provider::RefImage { bytes: vec![1, 2, 3], mime: "image/png".into() }),
            rig: true,
            animations: vec!["swim".into()],
            ..ModelRequest::default()
        };
        let out = meshy(&fake).model(&req).unwrap();
        assert!(!out.rigged && out.clips.is_empty());
        assert!(out.notes[0].contains("Pose estimation failed") && out.notes[0].contains("humanoids only"), "{:?}", out.notes);
        let sent = fake.sent.borrow();
        assert!(sent[0].2["image_url"].as_str().unwrap().starts_with("data:image/png;base64,AQID"));
        // A failed task reports Meshy's reason.
        let fake = Rc::new(Fake::new(vec![
            reply(202, json!({ "result": "p" })),
            reply(200, json!({ "id": "p", "status": "FAILED", "task_error": { "type": "invalid_input", "message": "moderation_blocked" }, "consumed_credits": 0 })),
        ]));
        let e = meshy(&fake).model(&ModelRequest { model: "meshy-7.1".into(), prompt: "x".into(), ..ModelRequest::default() }).unwrap_err();
        assert!(e.contains("moderation_blocked") && e.contains("task p"), "{e}");
    }
}

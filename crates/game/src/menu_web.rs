//! Serialises menu pages (`UiNode`s) for the HTML overlay and decodes its messages.
//!
//! Buttons carry an index into a table of actions; the page sends the index back with the
//! version of the table it was drawn from, so a click on a stale frame is ignored.

use bevy::prelude::*;
use serde_json::{Value, json};

use crate::menu::{FieldId, MenuAction};
use crate::ui::{Btn, UiAction, UiNode};

pub const OVERLAY_JS: &str = include_str!("menu_overlay.js");

/// One serialised menu frame and the actions its buttons refer to.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub json: String,
    pub actions: Vec<UiAction>,
}

fn rgba(c: Color) -> Value {
    let s = Srgba::from(c);
    json!([
        (s.red * 255.0).round() as u8,
        (s.green * 255.0).round() as u8,
        (s.blue * 255.0).round() as u8,
        (s.alpha * 100.0).round() / 100.0
    ])
}

fn style(b: Btn) -> &'static str {
    match b {
        Btn::Normal => "normal",
        Btn::Active => "active",
        Btn::Disabled => "disabled",
        Btn::Danger => "danger",
        Btn::Flat => "flat",
        Btn::Primary => "primary",
    }
}

fn field_name(id: FieldId) -> &'static str {
    match id {
        FieldId::Name => "name",
        FieldId::Seed => "seed",
    }
}

pub fn field_from_name(s: &str) -> Option<FieldId> {
    match s {
        "name" => Some(FieldId::Name),
        "seed" => Some(FieldId::Seed),
        _ => None,
    }
}

struct Ctx {
    actions: Vec<UiAction>,
}

impl Ctx {
    fn action(&mut self, a: &UiAction) -> usize {
        self.actions.push(a.clone());
        self.actions.len() - 1
    }

    fn list(&mut self, nodes: &[UiNode]) -> Vec<Value> {
        nodes.iter().filter_map(|n| self.node(n)).collect()
    }

    fn node(&mut self, n: &UiNode) -> Option<Value> {
        Some(match n {
            UiNode::Text(t, c, s) => json!({"k": "text", "t": t, "c": rgba(*c), "s": s}),
            UiNode::Button(t, a, st) => {
                json!({"k": "btn", "t": t, "a": self.action(a), "st": style(*st)})
            }
            UiNode::Row(items) => json!({"k": "row", "c": self.list(items)}),
            UiNode::Wrap(items) => json!({"k": "wrap", "c": self.list(items)}),
            UiNode::Col(items) => json!({"k": "col", "c": self.list(items)}),
            UiNode::Spread(items) => json!({"k": "spread", "c": self.list(items)}),
            UiNode::Columns(cols) => {
                let c: Vec<Vec<Value>> = cols.iter().map(|c| self.list(c)).collect();
                json!({"k": "columns", "c": c})
            }
            UiNode::Fixed(w, items) => json!({"k": "fixed", "w": w, "c": self.list(items)}),
            UiNode::Gap => json!({"k": "gap"}),
            UiNode::Divider => json!({"k": "div"}),
            UiNode::Spacer => json!({"k": "spacer"}),
            UiNode::Dot(c, s) => json!({"k": "dot", "c": rgba(*c), "s": s}),
            UiNode::Bar(f, c) => json!({"k": "bar", "f": f, "c": rgba(*c)}),
            UiNode::Card(items, accent) => {
                json!({"k": "card", "ac": rgba(*accent), "c": self.list(items)})
            }
            UiNode::Panel { width, items } => {
                json!({"k": "panel", "w": width, "c": self.list(items)})
            }
            UiNode::Overlay { left, items } => {
                json!({"k": "overlay", "left": left, "c": self.list(items)})
            }
            UiNode::BigButton {
                label,
                note,
                action,
                style: st,
            } => json!({
                "k": "big", "t": label, "note": note, "a": self.action(action), "st": style(*st)
            }),
            UiNode::Field {
                id,
                value,
                hint,
                focused: _,
            } => json!({"k": "field", "id": field_name(*id), "v": value, "hint": hint}),
            UiNode::Toggle(label, on, a) => {
                json!({"k": "toggle", "label": label, "on": on, "a": self.action(a)})
            }
            UiNode::Stepper {
                label,
                value,
                frac,
                dec,
                inc,
            } => json!({
                "k": "stepper", "label": label, "value": value, "frac": frac,
                "dec": self.action(dec), "inc": self.action(inc)
            }),
            // HUD-only widgets never appear on menu pages.
            _ => return None,
        })
    }
}

/// `page` is a short name the background page can react to (`main`, `load`, ...).
pub fn encode(page: &str, version: u64, nodes: &[UiNode]) -> Frame {
    let mut cx = Ctx { actions: vec![] };
    let list = cx.list(nodes);
    Frame {
        json: json!({"page": page, "v": version, "nodes": list}).to_string(),
        actions: cx.actions,
    }
}

/// A message from the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Ready,
    Back,
    Act { index: usize, version: u64 },
    Field(FieldId, String),
}

pub fn decode(msg: &str) -> Option<Incoming> {
    let v: Value = serde_json::from_str(msg).ok()?;
    match v.get("t")?.as_str()? {
        "ready" => Some(Incoming::Ready),
        "back" => Some(Incoming::Back),
        "act" => Some(Incoming::Act {
            index: v.get("i")?.as_u64()? as usize,
            version: v.get("v")?.as_u64()?,
        }),
        "field" => Some(Incoming::Field(
            field_from_name(v.get("id")?.as_str()?)?,
            v.get("v")?.as_str()?.to_string(),
        )),
        _ => None,
    }
}

impl Incoming {
    /// The UI action this message asks for, given the table of the frame it was drawn from.
    pub fn to_action(&self, frame: &Frame, current_version: u64) -> Option<UiAction> {
        match self {
            Incoming::Act { index, version } if *version == current_version => {
                frame.actions.get(*index).cloned()
            }
            Incoming::Back => Some(UiAction::Menu(MenuAction::Back)),
            Incoming::Field(id, v) => Some(UiAction::Menu(MenuAction::SetField(*id, v.clone()))),
            _ => None,
        }
    }
}

pub fn page_name(p: crate::menu::Page) -> &'static str {
    use crate::menu::Page;
    match p {
        Page::Main => "main",
        Page::NewGame => "new",
        Page::Load => "load",
        Page::Settings => "settings",
        Page::Pause => "pause",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::Page;

    fn sample() -> Vec<UiNode> {
        vec![UiNode::Overlay {
            left: false,
            items: vec![UiNode::Panel {
                width: 400.0,
                items: vec![
                    UiNode::title("Hello"),
                    UiNode::BigButton {
                        label: "Go".into(),
                        note: String::new(),
                        action: UiAction::Menu(MenuAction::StartNew),
                        style: Btn::Primary,
                    },
                    UiNode::Toggle(
                        "Sound".into(),
                        true,
                        UiAction::Menu(MenuAction::ToggleTutorial),
                    ),
                    UiNode::Field {
                        id: FieldId::Name,
                        value: "abc".into(),
                        hint: "name".into(),
                        focused: false,
                    },
                    UiNode::Icon(crate::icons::Icon::Close, Color::WHITE, 10.0),
                ],
            }],
        }]
    }

    #[test]
    fn frames_register_actions_in_order() {
        let f = encode("main", 3, &sample());
        assert_eq!(f.actions.len(), 2);
        assert_eq!(f.actions[0], UiAction::Menu(MenuAction::StartNew));
        let v: Value = serde_json::from_str(&f.json).expect("json");
        assert_eq!(v["page"], "main");
        assert_eq!(v["v"], 3);
        assert_eq!(v["nodes"][0]["k"], "overlay");
        // HUD-only nodes are skipped rather than breaking the page.
        let panel = &v["nodes"][0]["c"][0]["c"];
        assert_eq!(panel.as_array().expect("array").len(), 4);
    }

    #[test]
    fn clicks_resolve_against_the_matching_frame_only() {
        let f = encode("main", 7, &sample());
        let click = decode(r#"{"t":"act","i":0,"v":7}"#).expect("decode");
        assert_eq!(
            click.to_action(&f, 7),
            Some(UiAction::Menu(MenuAction::StartNew))
        );
        // The page redrew since: an old click must not fire the wrong button.
        assert_eq!(click.to_action(&f, 8), None);
        let out_of_range = decode(r#"{"t":"act","i":9,"v":7}"#).expect("decode");
        assert_eq!(out_of_range.to_action(&f, 7), None);
    }

    #[test]
    fn field_and_back_messages_decode() {
        assert_eq!(
            decode(r#"{"t":"field","id":"seed","v":"42"}"#),
            Some(Incoming::Field(FieldId::Seed, "42".into()))
        );
        assert_eq!(decode(r#"{"t":"back"}"#), Some(Incoming::Back));
        assert_eq!(decode(r#"{"t":"ready"}"#), Some(Incoming::Ready));
        assert_eq!(decode(r#"{"t":"field","id":"nope","v":""}"#), None);
        assert_eq!(decode("not json"), None);
    }

    #[test]
    fn page_names_are_stable() {
        assert_eq!(page_name(Page::NewGame), "new");
        assert_eq!(page_name(Page::Main), "main");
    }

    #[test]
    fn overlay_script_is_embedded() {
        assert!(OVERLAY_JS.contains("__bts_set"));
        assert!(OVERLAY_JS.contains("ipc.postMessage"));
    }
}

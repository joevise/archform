use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const CONTRACT_TYPES: [&str; 6] = [
    "sync-call",
    "event",
    "data-store",
    "tool-call",
    "model-io",
    "stream",
];

pub const KINDS: [&str; 6] = ["gateway", "service", "ai-runtime", "datastore", "bus", "app"];

pub fn is_contract(t: &str) -> bool {
    CONTRACT_TYPES.contains(&t)
}

pub fn contract_color(t: &str) -> &'static str {
    match t {
        "sync-call" => "#dc2626",
        "event" => "#f59e0b",
        "data-store" => "#7c3aed",
        "tool-call" => "#0ea5e9",
        "model-io" => "#10b981",
        "stream" => "#ec4899",
        _ => "#6b7280",
    }
}

pub fn contract_zh(t: &str) -> &'static str {
    match t {
        "sync-call" => "同步调用",
        "event" => "异步事件",
        "data-store" => "数据存储",
        "tool-call" => "工具调用",
        "model-io" => "模型I/O",
        "stream" => "流式",
        _ => "未知",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arch {
    pub version: u32,
    #[serde(default)]
    pub policies: HashMap<String, Policy>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub ptype: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdp: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default)]
    pub edges: Vec<String>,
    #[serde(default)]
    pub nodes: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<Port>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub id: String,
    pub ptype: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub etype: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    #[serde(default)]
    pub annotations: Vec<String>,
}

fn p(id: &str, ptype: &str, role: Option<&str>) -> Port {
    Port {
        id: id.to_string(),
        ptype: ptype.to_string(),
        role: role.map(|s| s.to_string()),
        spec: None,
    }
}

pub fn default_ports(kind: &str) -> Vec<Port> {
    match kind {
        "gateway" => vec![
            p("api", "sync-call", Some("server")),
            p("events", "event", None),
            p("chat", "stream", Some("server")),
        ],
        "service" => vec![
            p("invoke", "sync-call", Some("server")),
            p("emitted", "event", None),
            p("store", "data-store", None),
        ],
        "ai-runtime" => vec![
            p("invoke", "sync-call", Some("server")),
            p("llm", "model-io", None),
            p("tools", "tool-call", None),
            p("session", "data-store", None),
        ],
        "datastore" => vec![
            p("vault", "data-store", Some("server")),
            p("admin", "sync-call", Some("server")),
        ],
        "bus" => vec![p("topic", "event", Some("server"))],
        "app" => vec![p("backend", "sync-call", None), p("ui", "stream", None)],
        _ => vec![p("invoke", "sync-call", Some("server"))],
    }
}

impl Arch {
    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }

    pub fn resolve_port(&self, r: &str) -> Option<(&Component, &Port)> {
        let (cid, pid) = r.split_once('.')?;
        let c = self.component(cid)?;
        let port = c.ports.iter().find(|pp| pp.id == pid)?;
        Some((c, port))
    }
}

pub fn parse(yaml: &str) -> Result<Arch, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

pub fn to_yaml(a: &Arch) -> String {
    serde_yaml::to_string(a).unwrap_or_default()
}

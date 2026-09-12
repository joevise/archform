use crate::model::Arch;
use crate::validate::{self, VErr};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct SimStep {
    pub step: String,
    pub element: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SimResp {
    pub ok: bool,
    pub entry: String,
    pub steps: Vec<SimStep>,
    pub hops: usize,
}

fn step(kind: &str, element: &str, message: Option<String>) -> SimStep {
    SimStep {
        step: kind.to_string(),
        element: element.to_string(),
        message,
    }
}

fn pick_entry(a: &Arch) -> Option<String> {
    if let Some(c) = a
        .components
        .iter()
        .find(|c| c.kind == "gateway" && c.ports.iter().any(|p| p.ptype == "sync-call" && p.role.as_deref() == Some("server")))
    {
        let p = c
            .ports
            .iter()
            .find(|p| p.ptype == "sync-call" && p.role.as_deref() == Some("server"))
            .unwrap();
        return Some(format!("{}.{}", c.id, p.id));
    }
    a.components
        .iter()
        .find(|c| c.kind == "app")
        .and_then(|c| {
            c.ports
                .iter()
                .find(|p| p.role.as_deref() != Some("server"))
                .map(|p| format!("{}.{}", c.id, p.id))
        })
}

struct Sim {
    steps: Vec<SimStep>,
    hops: usize,
    ok: bool,
    visited: HashSet<String>,
}

const MAX_DEPTH: usize = 14;

fn dfs(a: &Arch, cid: &str, st: &mut Sim, depth: usize, errors: &[VErr]) {
    if !st.ok || depth >= MAX_DEPTH {
        return;
    }
    if !st.visited.insert(cid.to_string()) {
        return;
    }
    st.steps.push(step("process", cid, None));
    for e in a.edges.iter().filter(|e| e.from.split('.').next() == Some(cid)) {
        if let Some(err) = errors.iter().find(|x| x.element == e.id) {
            st.steps.push(step("stall", &e.id, Some(err.message.clone())));
            st.ok = false;
            return;
        }
        st.steps.push(step("hop", &e.id, None));
        st.hops += 1;
        if let Some(tc) = e.to.split('.').next() {
            if !st.visited.contains(tc) && a.component(tc).is_some() {
                dfs(a, tc, st, depth + 1, errors);
                if !st.ok {
                    return;
                }
            }
        }
    }
}

pub fn simulate(a: &Arch) -> SimResp {
    let (_, _, _, errors) = validate::validate(a);
    let entry = match pick_entry(a) {
        Some(e) => e,
        None => {
            return SimResp {
                ok: false,
                entry: String::new(),
                hops: 0,
                steps: vec![step(
                    "stall",
                    "-",
                    Some("找不到入口（需要 gateway 或 app 组件）".to_string()),
                )],
            }
        }
    };
    let entry_comp = entry.split('.').next().unwrap_or("").to_string();
    let mut st = Sim {
        steps: vec![step("enter", &entry, None)],
        hops: 0,
        ok: true,
        visited: HashSet::new(),
    };
    dfs(a, &entry_comp, &mut st, 0, &errors);
    if st.ok {
        st.steps.push(step(
            "done",
            &entry_comp,
            Some(format!("全链路 {} 跳逻辑通", st.hops)),
        ));
    }
    SimResp {
        ok: st.ok,
        entry,
        steps: st.steps,
        hops: st.hops,
    }
}

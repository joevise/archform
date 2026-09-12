use crate::model::{Arch, Edge};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct VErr {
    pub element: String,
    pub rule: String,
    pub message: String,
}

struct Ctx {
    pass: usize,
    fail: usize,
    errs: Vec<VErr>,
}

impl Ctx {
    fn ok(&mut self) {
        self.pass += 1;
    }
    fn bad(&mut self, element: &str, rule: &str, message: String) {
        self.fail += 1;
        self.errs.push(VErr {
            element: element.to_string(),
            rule: rule.to_string(),
            message,
        });
    }
}

fn edge_annotations(a: &Arch, e: &Edge) -> Vec<String> {
    let mut v = a.defaults.edges.clone();
    v.extend(e.annotations.clone());
    v
}

fn has_policy_type(a: &Arch, names: &[String], ptype: &str) -> bool {
    names
        .iter()
        .any(|n| a.policies.get(n).map(|p| p.ptype == ptype).unwrap_or(false))
}

pub fn validate(a: &Arch) -> (bool, usize, usize, Vec<VErr>) {
    let mut cx = Ctx {
        pass: 0,
        fail: 0,
        errs: Vec::new(),
    };

    // dup-id
    let mut seen: HashSet<&str> = HashSet::new();
    for c in &a.components {
        if seen.insert(c.id.as_str()) {
            cx.ok();
        } else {
            cx.bad(&c.id, "dup-id", format!("组件 id 重复: {}", c.id));
        }
    }
    for e in &a.edges {
        if seen.insert(e.id.as_str()) {
            cx.ok();
        } else {
            cx.bad(&e.id, "dup-id", format!("边 id 重复: {}", e.id));
        }
    }

    // per-edge rules
    for e in &a.edges {
        // self-loop
        if e.from == e.to {
            cx.bad(&e.id, "self-loop", format!("边 {} 自环: {}", e.id, e.from));
        } else {
            cx.ok();
        }

        // unknown-ref
        let from = a.resolve_port(&e.from);
        let to = a.resolve_port(&e.to);
        let (from, to) = match (from, to) {
            (Some(f), Some(t)) => {
                cx.ok();
                (f, t)
            }
            (f, t) => {
                let mut missing = Vec::new();
                if f.is_none() {
                    missing.push(e.from.clone());
                }
                if t.is_none() {
                    missing.push(e.to.clone());
                }
                cx.bad(
                    &e.id,
                    "unknown-ref",
                    format!("边 {} 引用不存在的端口: {}", e.id, missing.join(", ")),
                );
                continue;
            }
        };

        // port-mismatch
        if from.1.ptype == to.1.ptype && from.1.ptype == e.etype {
            cx.ok();
        } else {
            cx.bad(
                &e.id,
                "port-mismatch",
                format!(
                    "边 {} 类型 {} 与端口类型不符 ({}:{} -> {}:{})",
                    e.id, e.etype, e.from, from.1.ptype, e.to, to.1.ptype
                ),
            );
        }

        // event-consumer
        if e.etype == "event" {
            let topic_ok = e
                .spec
                .as_deref()
                .map(|s| s.contains("topic"))
                .unwrap_or(false);
            if from.1.ptype == "event" && topic_ok {
                cx.ok();
            } else {
                cx.bad(
                    &e.id,
                    "event-consumer",
                    format!("event 边 {} 必须从 event 口出发且 spec 含 topic", e.id),
                );
            }
        }

        // auth-propagation
        if e.etype == "sync-call" || e.etype == "stream" {
            let anns = edge_annotations(a, e);
            if has_policy_type(a, &anns, "auth") {
                cx.ok();
            } else {
                cx.bad(
                    &e.id,
                    "auth-propagation",
                    format!("边 {} 裸奔：缺 auth 注解（含 defaults 继承）", e.id),
                );
            }
        }

        // trace-coverage
        let anns = edge_annotations(a, e);
        if has_policy_type(a, &anns, "trace") {
            cx.ok();
        } else {
            cx.bad(
                &e.id,
                "trace-coverage",
                format!("边 {} 缺 trace 注解", e.id),
            );
        }
    }

    // auth-propagation: pdp existence
    for (name, pol) in &a.policies {
        if let Some(pdp) = &pol.pdp {
            if a.component(pdp).is_some() {
                cx.ok();
            } else {
                cx.bad(
                    name,
                    "auth-propagation",
                    format!("策略 {} 的 pdp 不存在: {}", name, pdp),
                );
            }
        }
    }

    // sandbox-floor
    for c in &a.components {
        if c.kind != "ai-runtime" {
            continue;
        }
        let sb = c
            .annotations
            .get("sandbox")
            .cloned()
            .or_else(|| a.defaults.nodes.get(&c.kind).and_then(|m| m.get("sandbox").cloned()));
        match sb.as_deref() {
            Some("container") | Some("seccomp") => cx.ok(),
            other => cx.bad(
                &c.id,
                "sandbox-floor",
                format!(
                    "ai-runtime {} sandbox 不达标: {}（需 container|seccomp）",
                    c.id,
                    other.unwrap_or("缺失")
                ),
            ),
        }
    }

    // sync-acyclic
    match find_sync_cycle(a) {
        None => cx.ok(),
        Some((element, path)) => cx.bad(
            &element,
            "sync-acyclic",
            format!("sync-call 子图存在环: {}", path.join("→")),
        ),
    }

    let all_ok = cx.fail == 0;
    (all_ok, cx.pass, cx.fail, cx.errs)
}

fn find_sync_cycle(a: &Arch) -> Option<(String, Vec<String>)> {
    // adjacency: comp -> Vec<(to_comp, edge_id, from_ref, to_ref)>
    let mut adj: HashMap<&str, Vec<(&str, &str, &str, &str)>> = HashMap::new();
    for e in &a.edges {
        if e.etype != "sync-call" {
            continue;
        }
        let (fc, _) = match a.resolve_port(&e.from) {
            Some(r) => r,
            None => continue,
        };
        let (tc, _) = match a.resolve_port(&e.to) {
            Some(r) => r,
            None => continue,
        };
        adj.entry(fc.id.as_str())
            .or_default()
            .push((tc.id.as_str(), e.id.as_str(), e.from.as_str(), e.to.as_str()));
    }

    let mut color: HashMap<&str, u8> = HashMap::new(); // 0/1=visiting 2=done
    let mut stack: Vec<(&str, &str)> = Vec::new(); // (comp, from_ref used to enter)

    fn dfs<'x>(
        u: &'x str,
        enter_from: &'x str,
        adj: &HashMap<&'x str, Vec<(&'x str, &'x str, &'x str, &'x str)>>,
        color: &mut HashMap<&'x str, u8>,
        stack: &mut Vec<(&'x str, &'x str)>,
    ) -> Option<(String, Vec<String>)> {
        color.insert(u, 1);
        stack.push((u, enter_from));
        if let Some(edges) = adj.get(u) {
            for &(v, eid, fref, tref) in edges {
                match color.get(v).copied().unwrap_or(0) {
                    1 => {
                        let mut path: Vec<String> = Vec::new();
                        let mut started = false;
                        for &(n, fr) in stack.iter() {
                            if n == v {
                                started = true;
                            }
                            if started {
                                path.push(fr.to_string());
                            }
                        }
                        path.push(fref.to_string());
                        path.push(tref.to_string());
                        return Some((eid.to_string(), path));
                    }
                    0 => {
                        if let Some(r) = dfs(v, fref, adj, color, stack) {
                            return Some(r);
                        }
                    }
                    _ => {}
                }
            }
        }
        stack.pop();
        color.insert(u, 2);
        None
    }

    for c in &a.components {
        if color.get(c.id.as_str()).copied().unwrap_or(0) == 0 {
            let enter = Box::leak(
                format!("{}.{}", c.id, c.ports.first().map(|p| p.id.as_str()).unwrap_or(""))
                    .into_boxed_str(),
            );
            if let Some(r) = dfs(c.id.as_str(), enter, &adj, &mut color, &mut stack) {
                return Some(r);
            }
        }
    }
    None
}

use archform::{generate, model, simulate, validate, web};

const SAMPLE: &str = include_str!("../examples/openmind.arch.yaml");

fn errs_of(a: &model::Arch, rule: &str) -> Vec<validate::VErr> {
    let (_, _, _, errs) = validate::validate(a);
    errs.into_iter().filter(|e| e.rule == rule).collect()
}

fn base_yaml() -> String {
    r#"
version: 1
policies:
  auth-jwt:
    ptype: auth
    scheme: jwt-bearer
    pdp: auth-service
  otel:
    ptype: trace
    scheme: opentelemetry
defaults:
  edges: [auth-jwt, otel]
  nodes:
    ai-runtime:
      sandbox: seccomp
components:
  - id: a
    kind: service
    x: 0
    y: 0
    annotations: {}
    ports:
      - id: invoke
        ptype: sync-call
        role: server
      - id: emitted
        ptype: event
      - id: store
        ptype: data-store
  - id: b
    kind: service
    x: 300
    y: 0
    annotations: {}
    ports:
      - id: invoke
        ptype: sync-call
        role: server
      - id: emitted
        ptype: event
      - id: store
        ptype: data-store
  - id: auth-service
    kind: service
    x: 600
    y: 0
    annotations: {}
    ports:
      - id: invoke
        ptype: sync-call
        role: server
edges: []
"#
    .to_string()
}

fn parse(y: &str) -> model::Arch {
    model::parse(y).expect("test yaml must parse")
}

#[test]
fn parse_roundtrip() {
    let a = parse(SAMPLE);
    let yaml2 = model::to_yaml(&a);
    let b = parse(&yaml2);
    assert_eq!(a.components.len(), b.components.len());
    assert_eq!(a.edges.len(), b.edges.len());
    assert_eq!(a.policies.len(), b.policies.len());
    assert_eq!(a.components[0].ports.len(), b.components[0].ports.len());
}

#[test]
fn port_mismatch_reported() {
    let y = base_yaml().replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.invoke\n    to: b.emitted\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "port-mismatch");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].element, "e1");
}

#[test]
fn unknown_ref_reported() {
    let y = base_yaml().replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.invoke\n    to: ghost.invoke\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "unknown-ref");
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("ghost.invoke"));
}

#[test]
fn sync_cycle_reported_with_path() {
    let y = base_yaml().replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.invoke\n    to: b.invoke\n    etype: sync-call\n    annotations: []\n  - id: e2\n    from: b.invoke\n    to: a.invoke\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "sync-acyclic");
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("→"), "cycle path reported: {}", errs[0].message);
    assert!(errs[0].message.contains("a.invoke"));
}

#[test]
fn auth_missing_reported() {
    let y = base_yaml().replace("  edges: [auth-jwt, otel]", "  edges: [otel]");
    let y = y.replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.invoke\n    to: b.invoke\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "auth-propagation");
    assert!(errs.iter().any(|e| e.message.contains("裸奔") && e.element == "e1"));
}

#[test]
fn pdp_missing_reported() {
    let y = base_yaml().replace("pdp: auth-service", "pdp: nope-service");
    let a = parse(&y);
    let errs = errs_of(&a, "auth-propagation");
    assert!(errs.iter().any(|e| e.message.contains("pdp 不存在")));
}

#[test]
fn sandbox_floor_reported() {
    // no defaults.nodes inheritance -> bare ai-runtime must fail
    let y = base_yaml().replace(
        "  nodes:\n    ai-runtime:\n      sandbox: seccomp",
        "  nodes: {}",
    );
    let y = y.replace(
        "  - id: auth-service",
        "  - id: brain\n    kind: ai-runtime\n    x: 0\n    y: 300\n    annotations: {}\n    ports:\n      - id: invoke\n        ptype: sync-call\n        role: server\n  - id: auth-service",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "sandbox-floor");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].element, "brain");
}

#[test]
fn defaults_inherited_pass() {
    // ai-runtime inherits sandbox from defaults.nodes; sync edge inherits auth+trace from defaults.edges
    let y = base_yaml().replace(
        "  - id: auth-service",
        "  - id: brain\n    kind: ai-runtime\n    x: 0\n    y: 300\n    annotations: {}\n    ports:\n      - id: invoke\n        ptype: sync-call\n        role: server\n  - id: auth-service",
    );
    let y = y.replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.invoke\n    to: brain.invoke\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    assert!(errs_of(&a, "sandbox-floor").is_empty());
    assert!(errs_of(&a, "auth-propagation").is_empty());
    assert!(errs_of(&a, "trace-coverage").is_empty());
}

#[test]
fn same_gate_violation_detected() {
    let y = base_yaml().replace(
        "  - id: auth-service",
        "  - id: myapp\n    kind: app\n    x: 0\n    y: 300\n    annotations: {}\n    ports:\n      - id: invoke\n        ptype: sync-call\n        role: client\n  - id: auth-service",
    );
    let y = y.replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: myapp.invoke\n    to: a.invoke\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "same-gate-iron");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].element, "e1");
    assert!(errs[0].message.contains("myapp") && errs[0].message.contains("同门铁律"));
}

#[test]
fn same_gate_ok() {
    let y = base_yaml().replace(
        "  - id: auth-service",
        "  - id: myapp\n    kind: app\n    x: 0\n    y: 300\n    annotations: {}\n    ports:\n      - id: invoke\n        ptype: sync-call\n        role: client\n  - id: gw\n    kind: gateway\n    x: 300\n    y: 300\n    annotations: {}\n    ports:\n      - id: api\n        ptype: sync-call\n        role: server\n  - id: auth-service",
    );
    let y = y.replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: myapp.invoke\n    to: gw.api\n    etype: sync-call\n    annotations: []",
    );
    let a = parse(&y);
    assert!(errs_of(&a, "same-gate-iron").is_empty());
}

#[test]
fn core_only_data_violation() {
    let y = base_yaml().replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.store\n    to: b.store\n    etype: data-store\n    annotations: []",
    );
    let a = parse(&y);
    let errs = errs_of(&a, "core-only-data");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].element, "e1");
    assert!(errs[0].message.contains("b") && errs[0].message.contains("单一持久层"));
}

#[test]
fn core_only_data_ok() {
    let y = base_yaml().replace(
        "  - id: auth-service",
        "  - id: core\n    kind: datastore\n    x: 0\n    y: 300\n    annotations: {}\n    ports:\n      - id: vault\n        ptype: data-store\n        role: server\n  - id: auth-service",
    );
    let y = y.replace(
        "edges: []",
        "edges:\n  - id: e1\n    from: a.store\n    to: core.vault\n    etype: data-store\n    annotations: []",
    );
    let a = parse(&y);
    assert!(errs_of(&a, "core-only-data").is_empty());
}

#[test]
fn generate_openapi_writes() {
    let a = parse(SAMPLE);
    let files = generate::generate(&a);
    let gw = files.iter().find(|(p, _)| p == "out/openapi/gateway.yaml");
    assert!(gw.is_some(), "gateway openapi generated");
    let (_, content) = gw.unwrap();
    assert!(content.contains("openapi: 3.0.3"));
    let me = files.iter().find(|(p, _)| p == "out/openapi/mindengine.yaml");
    assert!(me.is_some());
    assert!(me.unwrap().1.contains("/minds/{id}/invoke"));
}

#[test]
fn generate_ts_writes() {
    let a = parse(SAMPLE);
    let files = generate::generate(&a);
    let ts = files.iter().find(|(p, _)| p == "out/types.ts");
    assert!(ts.is_some());
    let (_, content) = ts.unwrap();
    assert!(content.contains("export interface Component"));
    assert!(content.contains("export interface Port"));
    assert!(content.contains("export interface Edge"));
    let mocks = files.iter().find(|(p, _)| p == "out/mocks.json");
    assert!(mocks.is_some());
    assert!(mocks.unwrap().1.contains("\"mocks\""));
}

#[tokio::test]
async fn api_validate_endpoint() {
    let dir = std::env::temp_dir().join(format!("archform-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = web::router(dir.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = format!("{{\"yaml\":{}}}", json_quote(SAMPLE));
    let req = format!(
        "POST /api/validate HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).await.unwrap();
    let text = String::from_utf8_lossy(&resp);
    assert!(text.starts_with("HTTP/1.1 200"), "got: {}", &text[..text.len().min(120)]);
    assert!(text.contains("\"ok\":true"), "body: {}", text);
    assert!(text.contains("\"fail\":0"));
    std::fs::remove_dir_all(&dir).ok();
}

fn json_quote(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

#[test]
fn sample_openmind_all_green() {
    let a = parse(SAMPLE);
    let (all_ok, pass, fail, errs) = validate::validate(&a);
    for e in &errs {
        eprintln!("[{}] {}: {}", e.rule, e.element, e.message);
    }
    assert!(all_ok, "sample must be all green");
    assert_eq!(fail, 0);
    assert!(pass > 0);
}

#[test]
fn simulate_all_green() {
    let a = parse(SAMPLE);
    let r = simulate::simulate(&a);
    for s in &r.steps {
        eprintln!("[{}] {} {:?}", s.step, s.element, s.message);
    }
    assert!(r.ok, "sample sim must be ok");
    assert_eq!(r.steps[0].step, "enter");
    assert_eq!(r.steps[0].element, "gateway.api");
    assert_eq!(r.steps.last().unwrap().step, "done");
    assert!(!r.steps.iter().any(|s| s.step == "stall"));
    assert!(r.hops > 0, "edges hopped");
}

#[test]
fn simulate_stall_on_broken() {
    let y = SAMPLE.replacen(
        "from: gateway.api, to: mindengine.invoke",
        "from: gateway.nope, to: mindengine.invoke",
        1,
    );
    let a = parse(&y);
    let r = simulate::simulate(&a);
    assert!(!r.ok);
    let stall = r.steps.iter().find(|s| s.step == "stall").expect("stall step");
    assert_eq!(stall.element, "e13");
    assert!(stall.message.as_deref().unwrap_or("").contains("e13"));
    assert_eq!(r.steps.last().unwrap().step, "stall", "sim terminates on stall");
}

#[test]
fn simulate_no_entry() {
    let a = parse("version: 1\ncomponents: []\nedges: []\n");
    let r = simulate::simulate(&a);
    assert!(!r.ok);
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.steps[0].step, "stall");
    assert_eq!(r.steps[0].element, "-");
    assert!(r.steps[0].message.as_deref().unwrap_or("").contains("入口"));
}

#[test]
fn simulate_cycle_safe() {
    let y = r#"version: 1
policies:
  auth-jwt: { ptype: auth, scheme: jwt-bearer }
  otel: { ptype: trace }
defaults:
  edges: [auth-jwt, otel]
  nodes: {}
components:
  - id: ga
    kind: gateway
    x: 0
    y: 0
    annotations: {}
    ports:
      - id: api
        ptype: sync-call
        role: server
  - id: b
    kind: service
    x: 300
    y: 0
    annotations: {}
    ports:
      - id: invoke
        ptype: sync-call
        role: server
edges:
  - id: e1
    from: ga.api
    to: b.invoke
    etype: sync-call
    spec: "openapi:paths=/x"
    annotations: []
  - id: e2
    from: b.invoke
    to: ga.api
    etype: sync-call
    spec: "openapi:paths=/y"
    annotations: []
"#;
    let a = parse(y);
    let r = simulate::simulate(&a);
    assert!(!r.steps.is_empty());
    assert!(r.hops <= 2, "hops bounded, got {}", r.hops);
    assert!(r.steps.len() <= 16, "steps bounded (no infinite loop): {}", r.steps.len());
}

#[test]
fn serialize_roundtrip() {
    let a = parse(SAMPLE);
    let yaml = model::to_yaml(&a);
    let b = parse(&yaml);
    assert_eq!(a, b, "graph -> yaml -> parse must be identity");
}

#[test]
fn component_name_optional() {
    let a = parse("version: 1\ncomponents:\n  - id: x\n    kind: service\n    x: 0\n    y: 0\nedges: []\n");
    assert!(a.components[0].name.is_none());
    assert_eq!(a.components[0].display_name(), "x", "display falls back to id");
    let b = parse(SAMPLE);
    assert_eq!(b.component("gateway").unwrap().name.as_deref(), Some("API网关·五扇门"));
    assert_eq!(b.component("gateway").unwrap().display_name(), "API网关·五扇门");
}

#[test]
fn serialize_policies_defaults() {
    let a = parse(SAMPLE);
    let yaml = model::to_yaml(&a);
    let b = parse(&yaml);
    assert_eq!(a.policies, b.policies, "policies preserved");
    assert_eq!(a.defaults, b.defaults, "defaults preserved");
    assert_eq!(b.policies["auth-jwt"].ptype, "auth");
    assert_eq!(b.policies["auth-jwt"].scheme.as_deref(), Some("jwt-bearer"));
    assert_eq!(b.policies["auth-jwt"].pdp.as_deref(), Some("auth-service"));
    assert_eq!(b.defaults.edges, vec!["auth-jwt".to_string(), "otel-trace".to_string()]);
    assert_eq!(
        b.defaults.nodes["ai-runtime"]["sandbox"], "container",
        "defaults.nodes inheritance preserved"
    );
}

#[test]
fn sample_crosscut_kind() {
    let a = parse(SAMPLE);
    let mut cc: Vec<&str> = a
        .components
        .iter()
        .filter(|c| c.kind == "crosscut")
        .map(|c| c.id.as_str())
        .collect();
    cc.sort();
    assert_eq!(cc, vec!["auth-service", "kms", "license", "otel-collector"]);
    assert!(model::KINDS.contains(&"crosscut"));
    let (ok, _, fail, errs) = validate::validate(&a);
    for e in &errs {
        eprintln!("[{}] {}: {}", e.rule, e.element, e.message);
    }
    assert!(ok, "sample with crosscut kinds must validate green");
    assert_eq!(fail, 0);
}

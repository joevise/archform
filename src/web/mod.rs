use crate::generate;
use crate::model::{self, Arch};
use crate::simulate::{self, SimResp, SimStep};
use crate::validate::{self, VErr};
use axum::extract::State;
use axum::response::Html;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

pub const SAMPLE_YAML: &str = include_str!("../../examples/openmind.arch.yaml");
const EDITOR_HTML: &str = include_str!("editor.html");

pub struct AppState {
    pub dir: PathBuf,
}

#[derive(Deserialize)]
pub struct YamlReq {
    yaml: String,
}

#[derive(Deserialize)]
pub struct SerializeReq {
    graph: Arch,
}

#[derive(Serialize)]
pub struct SerializeResp {
    yaml: String,
}

#[derive(Serialize)]
pub struct ArchResp {
    yaml: String,
    graph: Arch,
}

#[derive(Serialize)]
pub struct PutResp {
    ok: bool,
    errors: Vec<VErr>,
    graph: Option<Arch>,
}

#[derive(Serialize)]
pub struct ValidateResp {
    ok: bool,
    pass: usize,
    fail: usize,
    errors: Vec<VErr>,
}

#[derive(Serialize)]
pub struct GenResp {
    files: Vec<FileOut>,
}

#[derive(Serialize)]
pub struct FileOut {
    path: String,
    content: String,
}

fn yaml_err(msg: String) -> VErr {
    VErr {
        element: String::new(),
        rule: "yaml-parse".to_string(),
        message: msg,
    }
}

fn arch_file(dir: &PathBuf) -> PathBuf {
    dir.join("archform.yaml")
}

fn ensure_arch_file(dir: &PathBuf) -> std::io::Result<()> {
    let f = arch_file(dir);
    if !f.exists() {
        std::fs::write(&f, SAMPLE_YAML)?;
    }
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(EDITOR_HTML)
}

async fn get_arch(State(st): State<Arc<AppState>>) -> Json<ArchResp> {
    let _ = ensure_arch_file(&st.dir);
    let yaml = std::fs::read_to_string(arch_file(&st.dir)).unwrap_or_else(|_| SAMPLE_YAML.to_string());
    let graph = model::parse(&yaml)
        .unwrap_or_else(|_| model::parse(SAMPLE_YAML).expect("embedded sample must parse"));
    Json(ArchResp { yaml, graph })
}

async fn put_arch(State(st): State<Arc<AppState>>, Json(req): Json<YamlReq>) -> Json<PutResp> {
    match model::parse(&req.yaml) {
        Err(e) => Json(PutResp {
            ok: false,
            errors: vec![yaml_err(format!("YAML 解析失败: {}", e))],
            graph: None,
        }),
        Ok(graph) => {
            let _ = std::fs::write(arch_file(&st.dir), &req.yaml);
            let (ok, _, _, errors) = validate::validate(&graph);
            Json(PutResp {
                ok,
                errors,
                graph: Some(graph),
            })
        }
    }
}

async fn post_serialize(Json(req): Json<SerializeReq>) -> Json<SerializeResp> {
    Json(SerializeResp {
        yaml: model::to_yaml(&req.graph),
    })
}

async fn post_validate(Json(req): Json<YamlReq>) -> Json<ValidateResp> {
    match model::parse(&req.yaml) {
        Err(e) => Json(ValidateResp {
            ok: false,
            pass: 0,
            fail: 1,
            errors: vec![yaml_err(format!("YAML 解析失败: {}", e))],
        }),
        Ok(graph) => {
            let (ok, pass, fail, errors) = validate::validate(&graph);
            Json(ValidateResp {
                ok,
                pass,
                fail,
                errors,
            })
        }
    }
}

async fn post_generate(State(st): State<Arc<AppState>>, Json(req): Json<YamlReq>) -> Json<GenResp> {
    let files = match model::parse(&req.yaml) {
        Err(_) => Vec::new(),
        Ok(graph) => generate::generate(&graph)
            .into_iter()
            .map(|(path, content)| {
                let full = st.dir.join(&path);
                if let Some(parent) = full.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&full, &content);
                FileOut { path, content }
            })
            .collect(),
    };
    Json(GenResp { files })
}

async fn post_simulate(Json(req): Json<YamlReq>) -> Json<SimResp> {
    match model::parse(&req.yaml) {
        Err(e) => Json(SimResp {
            ok: false,
            entry: String::new(),
            hops: 0,
            steps: vec![SimStep {
                step: "stall".to_string(),
                element: "-".to_string(),
                message: Some(format!("YAML 解析失败: {}", e)),
            }],
        }),
        Ok(graph) => Json(simulate::simulate(&graph)),
    }
}

pub fn router(dir: PathBuf) -> Router {
    let st = Arc::new(AppState { dir });
    Router::new()
        .route("/", get(index))
        .route("/api/arch", get(get_arch))
        .route("/api/arch", put(put_arch))
        .route("/api/serialize", post(post_serialize))
        .route("/api/validate", post(post_validate))
        .route("/api/simulate", post(post_simulate))
        .route("/api/generate", post(post_generate))
        .with_state(st)
}

pub async fn serve(port: u16, dir: PathBuf) -> std::io::Result<()> {
    let _ = ensure_arch_file(&dir);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("archform serving http://{} (dir: {})", addr, dir.display());
    axum::serve(listener, router(dir)).await
}

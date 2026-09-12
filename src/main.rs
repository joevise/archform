use archform::{generate, model, validate, web};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: archform check [file] | gen [file] | serve [--port 7920] [--dir .]");
    std::process::exit(2);
}

fn read_arch(file: &str) -> model::Arch {
    let yaml = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {}", file, e);
        std::process::exit(1);
    });
    model::parse(&yaml).unwrap_or_else(|e| {
        eprintln!("YAML parse error in {}: {}", file, e);
        std::process::exit(1);
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("check") => {
            let file = args.get(1).cloned().unwrap_or_else(|| "archform.yaml".to_string());
            let a = read_arch(&file);
            let (ok, pass, fail, errs) = validate::validate(&a);
            for e in &errs {
                println!("❌ [{}] {}: {}", e.rule, e.element, e.message);
            }
            println!("✅ pass: {}  ❌ fail: {}", pass, fail);
            if ok {
                println!("ALL GREEN");
            } else {
                std::process::exit(1);
            }
        }
        Some("gen") => {
            let file = args.get(1).cloned().unwrap_or_else(|| "archform.yaml".to_string());
            let a = read_arch(&file);
            let base = PathBuf::from(&file)
                .parent()
                .map(|p| p.to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| PathBuf::from("."));
            for (path, content) in generate::generate(&a) {
                let full = base.join(&path);
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&full, &content).expect("write generated file");
                println!("wrote {}", full.display());
            }
        }
        Some("serve") => {
            let mut port: u16 = 7920;
            let mut dir = PathBuf::from(".");
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--port" => {
                        i += 1;
                        port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(7920);
                    }
                    "--dir" => {
                        i += 1;
                        dir = PathBuf::from(args.get(i).cloned().unwrap_or_else(|| ".".to_string()));
                    }
                    _ => usage(),
                }
                i += 1;
            }
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(web::serve(port, dir)).expect("serve");
        }
        _ => usage(),
    }
}

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Error};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use clap::Parser;
use serde::{Deserialize, Serialize};
use task_maker_dag::{ExecutionDAG, ExecutionStatus, File};
use task_maker_lang::{LanguageManager, SourceFile};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use crate::sandbox::ToolsSandboxRunner;

#[derive(Parser, Debug, Clone)]
pub struct EvalServerOpt {
    /// Address to bind the server on
    #[clap(long, default_value = "127.0.0.1:3000")]
    pub addr: String,

    /// Require authentication for the API endpoints.
    /// If true, the server will expect an "X-OII-AUTH" header with a token for each request.
    ///
    /// The token is meaningless and is only used to prevent unauthorized access. You can set it to
    /// any value you want.
    #[clap(long)]
    pub require_header: bool,

    /// List of allowed languages (names). If empty, all languages are allowed.
    #[clap(long)]
    pub allowed_languages: Vec<String>,

    /// Maximum CPU time limit (seconds)
    #[clap(long, default_value = "10.0")]
    pub max_time_limit: f64,

    /// Maximum memory limit (MB)
    #[clap(long, default_value = "512")]
    pub max_memory_limit: u64,

    /// Compilation CPU time limit (seconds)
    #[clap(long, default_value = "60.0")]
    pub compilation_time_limit: f64,

    /// Compilation memory limit (MB)
    #[clap(long, default_value = "1024")]
    pub compilation_memory_limit: u64,

    #[clap(flatten, next_help_heading = Some("STORAGE"))]
    pub storage: crate::StorageOpt,
}

struct AppState {
    require_header: bool,
    allowed_languages: HashSet<String>,
    opt: EvalServerOpt,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Binary(Vec<u8>),
}

impl Content {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Content::Text(s) => s.into_bytes(),
            Content::Binary(b) => b,
        }
    }
}

impl From<Vec<u8>> for Content {
    fn from(v: Vec<u8>) -> Self {
        match String::from_utf8(v) {
            Ok(s) => Content::Text(s),
            Err(e) => Content::Binary(e.into_bytes()),
        }
    }
}

impl From<Bytes> for Content {
    fn from(b: Bytes) -> Self {
        Content::from(b.to_vec())
    }
}

#[derive(Debug, Deserialize)]
struct SourceFileContent {
    #[serde(deserialize_with = "validate_filename")]
    name: String,
    content: Content,
}

#[derive(Debug, Deserialize)]
struct EvalRequest {
    files: Vec<SourceFileContent>,
    #[serde(deserialize_with = "validate_filename")]
    main_filename: String,
    input: Content,
    time_limit: Option<f64>,
    memory_limit: Option<u64>,
    language: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecutionResult {
    status: String,
    exit_code: u32,
    stdout: Content,
    stderr: Content,
    time: f64,
    memory: f64,
}

#[derive(Debug, Serialize)]
struct EvalResponse {
    execution: Option<ExecutionResult>,
    compilation: Option<ExecutionResult>,
}

#[derive(Debug, Serialize)]
struct LanguageInfo {
    name: String,
    extensions: Vec<String>,
}

#[tokio::main]
pub async fn main_eval_server(opt: EvalServerOpt) -> Result<(), Error> {
    let allowed_languages = opt.allowed_languages.iter().cloned().collect();
    let state = Arc::new(AppState {
        require_header: opt.require_header,
        allowed_languages,
        opt,
    });

    let app = Router::new()
        .route("/evaluate", post(evaluate))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .route("/languages", get(list_languages))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr = &state.opt.addr;
    let listener = TcpListener::bind(addr)
        .await
        .context("Failed to bind address")?;
    info!("Eval server listening on {}", addr);
    axum::serve(listener, app).await.context("Server error")
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.require_header {
        let auth_header = req
            .headers()
            .get("X-OII-AUTH")
            .and_then(|h| h.to_str().ok());

        match auth_header {
            Some(token) => {
                // Token is valid, continue to the next handler
                let token_str = token.to_string();
                req.extensions_mut().insert(token_str);
                Ok(next.run(req).await)
            }
            _ => {
                warn!("Unauthorized request");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    } else {
        // No tokens file provided, allow all requests
        Ok(next.run(req).await)
    }
}

async fn list_languages(State(state): State<Arc<AppState>>) -> Json<Vec<LanguageInfo>> {
    let langs = LanguageManager::languages();
    let info = langs
        .iter()
        .filter(|l| {
            state.allowed_languages.is_empty() || state.allowed_languages.contains(l.name())
        })
        .map(|l| LanguageInfo {
            name: l.name().to_string(),
            extensions: l.extensions().into_iter().map(|e| e.to_string()).collect(),
        })
        .collect();
    Json(info)
}

fn validate_filename<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.contains('/') || s.contains('\\') || s.contains('\0') || s == "." || s == ".." {
        return Err(serde::de::Error::custom(format!("Invalid filename: {}", s)));
    }
    Ok(s)
}

async fn evaluate(
    State(state): State<Arc<AppState>>,
    extensions: axum::http::Extensions,
    Json(payload): Json<EvalRequest>,
) -> Result<Json<EvalResponse>, (StatusCode, String)> {
    let token = extensions
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "none".to_string());
    debug!(
        "Evaluation request from token {}: main_filename={}, files={}",
        token,
        payload.main_filename,
        payload.files.len()
    );

    let temp_dir =
        TempDir::new().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    for file in payload.files {
        let file_path = temp_dir.path().join(&file.name);
        std::fs::write(&file_path, file.content.into_bytes())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let main_path = temp_dir.path().join(&payload.main_filename);
    if !main_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Main filename not found in files list".to_string(),
        ));
    }

    let mut dag = ExecutionDAG::new();
    let source_file = if let Some(lang_name) = &payload.language {
        let lang = LanguageManager::from_name(lang_name)
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "Unknown language".to_string()))?;
        SourceFile {
            path: main_path.clone(),
            base_path: temp_dir.path().to_owned(),
            language: lang,
            executable: Arc::new(Mutex::new(None)),
            grader_map: None,
            copy_exe: false,
            write_bin_to: None,
            link_static: false,
        }
    } else {
        SourceFile::new(&main_path, temp_dir.path(), None, None::<PathBuf>).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "Could not detect language".to_string(),
            )
        })?
    };

    // Check if the language is allowed
    if !state.allowed_languages.is_empty()
        && !state
            .allowed_languages
            .contains(source_file.language.name())
    {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Language '{}' is not allowed", source_file.language.name()),
        ));
    }

    let (comp_uuid, mut exec) = source_file
        .execute(&mut dag, "Evaluation", vec![])
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(cuuid) = comp_uuid {
        if let Some(comp_group) = dag.data.execution_groups.get_mut(&cuuid) {
            for exec in &mut comp_group.executions {
                exec.limits.cpu_time(state.opt.compilation_time_limit);
                exec.limits
                    .memory(state.opt.compilation_memory_limit * 1024);
            }
        }
    }

    // Set limits
    if let Some(tl) = payload.time_limit {
        if tl > state.opt.max_time_limit {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Time limit {:.2}s exceeds maximum allowed ({:.2}s)",
                    tl, state.opt.max_time_limit
                ),
            ));
        }
    }
    if let Some(ml) = payload.memory_limit {
        if ml > state.opt.max_memory_limit {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Memory limit {}MB exceeds maximum allowed ({}MB)",
                    ml, state.opt.max_memory_limit
                ),
            ));
        }
    }

    exec.limits
        .cpu_time(payload.time_limit.unwrap_or(state.opt.max_time_limit));
    exec.limits
        .memory(payload.memory_limit.unwrap_or(state.opt.max_memory_limit) * 1024);

    // Input file
    let input_file = File::new("input.txt");
    dag.provide_content(input_file.clone(), payload.input.into_bytes());
    exec.stdin(input_file);

    // Capture output
    let stdout = exec.capture_stdout(None);
    let stderr = exec.capture_stderr(None);

    let exec_uuid = dag.add_execution(exec);

    // Results capture
    let result_capture = Arc::new(Mutex::new(None));
    let comp_result_capture = Arc::new(Mutex::new(None));
    let stdout_capture = Arc::new(Mutex::new(Bytes::new()));
    let stderr_capture = Arc::new(Mutex::new(Bytes::new()));
    let comp_stdout_capture = Arc::new(Mutex::new(Bytes::new()));
    let comp_stderr_capture = Arc::new(Mutex::new(Bytes::new()));

    {
        let result_capture = result_capture.clone();
        dag.on_execution_done(&exec_uuid, move |results| {
            if let Some(result) = results.first() {
                *result_capture.lock().unwrap() = Some(result.clone());
            }
            Ok(())
        });

        let stdout_capture = stdout_capture.clone();
        dag.get_file_content(&stdout, 10 * 1024 * 1024, move |content| {
            *stdout_capture.lock().unwrap() = Bytes::from(content);
            Ok(())
        });

        let stderr_capture = stderr_capture.clone();
        dag.get_file_content(&stderr, 10 * 1024 * 1024, move |content| {
            *stderr_capture.lock().unwrap() = Bytes::from(content);
            Ok(())
        });

        if let Some(cuuid) = comp_uuid {
            let comp_result_capture = comp_result_capture.clone();
            let comp_stdout_capture = comp_stdout_capture.clone();
            let comp_stderr_capture = comp_stderr_capture.clone();

            dag.on_execution_done(&cuuid, move |results| {
                if let Some(res) = results.first() {
                    *comp_result_capture.lock().unwrap() = Some(res.clone());
                    if let Some(out) = &res.stdout {
                        *comp_stdout_capture.lock().unwrap() = Bytes::from(out.clone());
                    }
                    if let Some(err) = &res.stderr {
                        *comp_stderr_capture.lock().unwrap() = Bytes::from(err.clone());
                    }
                }
                Ok(())
            });
        }
    }

    let store_dir = state.opt.storage.store_dir();
    let sandbox_path = store_dir.join("eval-sandboxes");
    let max_cache = state.opt.storage.max_cache * 1024 * 1024;
    let min_cache = state.opt.storage.min_cache * 1024 * 1024;

    tokio::task::spawn_blocking(move || {
        task_maker_exec::eval_dag_locally(
            dag,
            store_dir,
            1,
            sandbox_path,
            max_cache,
            min_cache,
            ToolsSandboxRunner::default(),
        );
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let final_result = result_capture.lock().unwrap().take();

    let compilation = comp_result_capture
        .lock()
        .unwrap()
        .take()
        .map(|r| ExecutionResult {
            status: format!("{:?}", r.status),
            exit_code: match r.status {
                ExecutionStatus::ReturnCode(c) => c,
                _ => 0,
            },
            stdout: Content::from(comp_stdout_capture.lock().unwrap().clone()),
            stderr: Content::from(comp_stderr_capture.lock().unwrap().clone()),
            time: r.resources.cpu_time,
            memory: r.resources.memory as f64 / 1024.0,
        });

    let execution = if let Some(res) = final_result {
        Some(ExecutionResult {
            status: format!("{:?}", res.status),
            exit_code: match res.status {
                ExecutionStatus::ReturnCode(c) => c,
                _ => 0,
            },
            stdout: Content::from(stdout_capture.lock().unwrap().clone()),
            stderr: Content::from(stderr_capture.lock().unwrap().clone()),
            time: res.resources.cpu_time,
            memory: res.resources.memory as f64 / 1024.0,
        })
    } else {
        None
    };

    let response = EvalResponse {
        execution,
        compilation,
    };

    match (&response.execution, &response.compilation) {
        (Some(exec), Some(comp)) => {
            info!(
                "Evaluation finished for token {}: status={}, time={:.3}s, memory={:.2}MiB (compilation: status={}, time={:.3}s, memory={:.2}MiB)",
                token, exec.status, exec.time, exec.memory, comp.status, comp.time, comp.memory
            );
        }
        (Some(exec), None) => {
            info!(
                "Evaluation finished for token {}: status={}, time={:.3}s, memory={:.2}MiB",
                token, exec.status, exec.time, exec.memory
            );
        }
        (None, Some(comp)) => {
            info!(
                "Evaluation finished for token {}: no execution result, compilation: status={}, time={:.3}s, memory={:.2}MiB",
                token, comp.status, comp.time, comp.memory
            );
        }
        (None, None) => {
            info!(
                "Evaluation finished for token {}: no execution result and no compilation result",
                token
            );
        }
    }

    Ok(Json(response))
}

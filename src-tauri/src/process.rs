use pragma_parser::{
    parser::Parser,
    storage::{
        update_session_attachments as db_update_session_attachments, update_session_usage,
        SqliteSink,
    },
    types::{Atom, AtomSink, AtomType, OutputSource},
};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::protocol::PragmaEmitState;

// ---------------------------------------------------------------------------
// UTF-16 LE/BE / UTF-8 BOM / plain UTF-8 decoder
// ---------------------------------------------------------------------------

pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let shorts: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&shorts).to_string();
    }
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let shorts: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&shorts).to_string();
    }
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    String::from_utf8_lossy(bytes).to_string()
}

// ---------------------------------------------------------------------------
// Binary path validation
// ---------------------------------------------------------------------------

pub fn validate_binary_path(binary: &str) -> Result<(), String> {
    if binary.trim().is_empty() {
        return Err("binary path is empty".to_string());
    }
    let p = std::path::Path::new(binary);
    if p.is_absolute() {
        if p.components().any(|c| c == std::path::Component::ParentDir) {
            return Err(format!("binary path contains '..': {binary}"));
        }
        if !p.exists() {
            return Err(format!("binary not found: {binary}"));
        }
        if !p.is_file() {
            return Err(format!("binary path is not a file: {binary}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Structured error emission
// ---------------------------------------------------------------------------

pub fn emit_error(
    app: &AppHandle,
    code: &'static str,
    message: impl Into<String>,
    raw: impl std::fmt::Display,
) {
    let _ = app.emit("session:debug", format!("[error:{code}] {raw}"));
    let _ = app.emit(
        "session:error",
        serde_json::json!({ "code": code, "message": message.into() }),
    );
}

// ---------------------------------------------------------------------------
// Shared streaming core — used by run_pragma and send_control commands
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn stream_claude(
    app: AppHandle,
    session_id: Uuid,
    binary: String,
    extra_args: Vec<String>,
    working_dir: Option<String>,
    config_dir: Option<String>,
    db_command: Option<String>,
    db_title: Option<String>,
    attachments: Vec<String>,
    resume: bool,
    skip_permissions: bool,
    sessions: Arc<TokioMutex<HashMap<String, tokio::process::Child>>>,
) {
    let sid = session_id.to_string();
    let mut parser = Parser::new(session_id);
    let mut pragma_state = PragmaEmitState::new();
    let mut sqlite: Option<SqliteSink> = if resume {
        SqliteSink::open_existing(session_id).ok()
    } else {
        db_command.as_deref().and_then(|cmd| {
            SqliteSink::new(
                session_id,
                Some(cmd),
                db_title.as_deref(),
                working_dir.as_deref(),
            )
            .ok()
        })
    };
    if sqlite.is_some() && !attachments.is_empty() {
        let _ = db_update_session_attachments(&sid, &attachments);
    }

    let mut cmd = tokio::process::Command::new(&binary);
    let mut base_args = vec!["--output-format", "stream-json", "--verbose", "-p"];
    if skip_permissions {
        base_args.push("--dangerously-skip-permissions");
    }
    cmd.args(&base_args)
        .args(&extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    if let Some(ref dir) = working_dir {
        cmd.current_dir(dir);
        let pragmadocs = std::path::Path::new(dir).join(".pragmadocs");
        let _ = std::fs::create_dir_all(&pragmadocs);
        let pragmadocs_str = pragmadocs.to_string_lossy().into_owned();
        cmd.args(["--add-dir", &pragmadocs_str]);
    }
    if let Some(ref dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }

    if let Err(e) = validate_binary_path(&binary) {
        emit_error(
            &app,
            "ERR_BINARY_INVALID",
            "Binary path is invalid. Check the profile settings.",
            e,
        );
        return;
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit_error(
                &app,
                "ERR_SPAWN_FAILED",
                "Could not start the Claude binary.",
                e,
            );
            return;
        }
    };

    let stdout = child.stdout.take().expect("stdout pipe missing");
    let stderr = child.stderr.take().expect("stderr pipe missing");

    sessions.lock().await.insert(sid.clone(), child);

    let app_err = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut buf = Vec::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf).await;
        if !buf.is_empty() {
            let text = decode_bytes(&buf);
            let text = text.trim();
            if !text.is_empty() {
                let _ = app_err.emit("session:debug", format!("[stderr] {text}"));
            }
        }
    });

    let mut lines_iter = BufReader::new(stdout).lines();
    let mut batch: Vec<Atom> = Vec::new();
    let mut last_emit = std::time::Instant::now();
    let mut line_count = 0usize;

    while let Ok(Some(raw_line)) = lines_iter.next_line().await {
        let line = raw_line.trim_end_matches('\r').to_string();
        let _ = app.emit("session:debug", format!("[line] {:?}", line));
        line_count += 1;

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            let usage_obj = match v["type"].as_str() {
                Some("result") => Some(&v["usage"]),
                Some("assistant") => Some(&v["message"]["usage"]),
                _ => None,
            };
            if let Some(u) = usage_obj {
                if u.is_object() {
                    let model = v["message"]["model"].as_str().unwrap_or("").to_string();
                    let cost = v["total_cost_usd"].as_f64();
                    let inp = u["input_tokens"].as_u64().unwrap_or(0);
                    let out = u["output_tokens"].as_u64().unwrap_or(0);
                    let cr = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
                    let cw = u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                    let _ = app.emit(
                        "session:usage",
                        serde_json::json!({
                            "inputTokens":   inp,
                            "outputTokens":  out,
                            "cacheReadTokens":     cr,
                            "cacheWriteTokens":    cw,
                            "model":         model,
                            "totalCostUsd":  cost,
                        }),
                    );
                    let _ = update_session_usage(&sid, inp, out, cr, cw, cost);
                }
            }
        }

        let atoms = parser.feed_line(&line, OutputSource::Stdout);
        batch.extend(atoms);

        let now = std::time::Instant::now();
        if now.duration_since(last_emit).as_millis() >= 50 && !batch.is_empty() {
            for atom in &batch {
                pragma_state.process_atom(&app, atom);
                if atom.atom_type == AtomType::Error {
                    emit_error(
                        &app,
                        "ERR_CLAUDE_RUNTIME",
                        atom.content.clone(),
                        &atom.content,
                    );
                }
            }
            let _ = app.emit("atoms:batch", &batch);
            if let Some(ref mut db) = sqlite {
                for atom in &batch {
                    db.push(atom.clone()).ok();
                }
            }
            batch.clear();
            last_emit = now;
        }
    }

    let _ = app.emit(
        "session:debug",
        format!("[stdout] {line_count} lines received"),
    );

    batch.extend(parser.finalize(&OutputSource::Stdout));
    if !batch.is_empty() {
        for atom in &batch {
            pragma_state.process_atom(&app, atom);
            if atom.atom_type == AtomType::Error {
                let _ = app.emit("session:error", atom.content.clone());
            }
        }
        let _ = app.emit("atoms:batch", &batch);
        if let Some(ref mut db) = sqlite {
            for atom in &batch {
                db.push(atom.clone()).ok();
            }
        }
    }

    let last_note = batch
        .iter()
        .chain(std::iter::once(&batch).flatten())
        .rfind(|a| a.atom_type == AtomType::AgentNote)
        .map(|a| a.content.as_str());
    pragma_state.flush_pending(&app, last_note);

    if let Some(ref mut db) = sqlite {
        db.finalize().ok();
    }

    let _ = app.emit("session:complete", &sid);

    if let Some(mut c) = sessions.lock().await.remove(&sid) {
        let _ = c.wait().await;
    }
}

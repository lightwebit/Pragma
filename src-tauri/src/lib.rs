mod files;
mod process;
mod protocol;
mod trust;
mod usage;

use files::{do_copy_to_pragmadocs, sanitize_title};
use pragma_parser::{
    settings::{
        detect_claude_binary, load_prompt, load_settings, load_settings_with_autodetect,
        resolve_export_dir, save_settings, Settings,
    },
    storage::{
        delete_session as db_delete_session, duplicate_session as db_duplicate_session,
        export_session_json, list_saved_sessions, load_session_atoms, mark_session_saved,
        save_atom as db_save_atom, search_sessions as db_search_sessions,
        update_session_attachments as db_update_session_attachments, update_session_title,
        write_export_file, write_markdown_file, SessionInfo,
    },
    types::Atom,
};
use process::stream_claude;
use protocol::{PRAGMA_PROTOCOL, PRAGMA_STEP_BY_STEP};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;
use trust::TrustedDirs;
use usage::get_claude_usage;
use uuid::Uuid;

struct ActiveSessions(Arc<TokioMutex<HashMap<String, tokio::process::Child>>>);

// ---------------------------------------------------------------------------
// Session commands
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn run_pragma(
    app: AppHandle,
    prompt: String,
    binary: String,
    config_dir: Option<String>,
    working_dir: Option<String>,
    profile_label: Option<String>,
    title: Option<String>,
    model: Option<String>,
    attachments: Option<Vec<String>>,
    existing_session_id: Option<String>,
    step_by_step: Option<bool>,
    sessions: tauri::State<'_, ActiveSessions>,
    trusted_dirs: tauri::State<'_, TrustedDirs>,
) -> Result<String, String> {
    let (session_id, session_id_str, resume) = if let Some(ref existing) = existing_session_id {
        let uuid = existing
            .parse::<Uuid>()
            .map_err(|e| format!("session_id non valido: {e}"))?;
        (uuid, existing.clone(), true)
    } else {
        let uuid = Uuid::new_v4();
        let s = uuid.to_string();
        (uuid, s, false)
    };

    let mut extra_args: Vec<String> = if resume {
        vec!["--resume".to_string(), session_id_str.clone()]
    } else {
        vec!["--session-id".to_string(), session_id_str.clone()]
    };

    if model.as_deref() == Some("opus") {
        extra_args.push("--model".to_string());
        extra_args.push("claude-opus-4-7".to_string());
    }

    extra_args.push("--append-system-prompt".to_string());
    let protocol_text = if step_by_step.unwrap_or(false) {
        format!("{}\n{}", PRAGMA_PROTOCOL.trim(), PRAGMA_STEP_BY_STEP.trim())
    } else {
        PRAGMA_PROTOCOL.trim().to_string()
    };
    extra_args.push(protocol_text);

    if let Some(ref label) = profile_label {
        if let Some(sp) = load_prompt(label) {
            extra_args.push("--append-system-prompt".to_string());
            extra_args.push(sp);
        }
        if let Ok(settings) = pragma_parser::settings::load_settings() {
            if let Some(profile) = settings.profiles.iter().find(|p| &p.label == label) {
                let lang = profile
                    .language
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("English");
                extra_args.push("--append-system-prompt".to_string());
                extra_args.push(format!("Always respond in: {lang}."));
            }
        }
    }

    let wrapped_prompt = if resume {
        prompt.clone()
    } else {
        format!(
            "<pragma>\n\
             MANDATORY: Follow the Pragma protocol for your ENTIRE response.\n\
             - Start with ANALYSIS: (required, always)\n\
             - If unclear: QUESTIONS: / AWAITING_ANSWERS → stop\n\
             - If clear: PLAN: (numbered steps) / AWAITING_CONFIRMATION → stop, do NOT execute yet\n\
             - After confirmation: STEP_COMPLETE: N for each step, then REPORT:, TESTS:, CLOSING: / AWAITING_CLOSE\n\
             - Write ALL marker keywords in English (ANALYSIS:, PLAN:, etc.) even if you reply in another language\n\
             </pragma>\n\n\
             {prompt}"
        )
    };
    extra_args.push(wrapped_prompt);

    let skip_permissions = match &working_dir {
        None => true,
        Some(dir) => {
            if !trusted_dirs.contains(dir) {
                trusted_dirs.insert(dir.clone());
            }
            true
        }
    };

    let debug_args: Vec<String> = extra_args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.len() > 120 {
                format!("[arg{}] {}…", i, &a[..80])
            } else {
                format!("[arg{}] {}", i, a)
            }
        })
        .collect();
    let _ = app.emit(
        "session:debug",
        format!("[run_pragma] args: {}", debug_args.join(" | ")),
    );

    let app2 = app.clone();
    let sid_str = session_id_str.clone();
    let sessions_arc = Arc::clone(&sessions.0);

    tauri::async_runtime::spawn(async move {
        stream_claude(
            app2,
            session_id,
            binary,
            extra_args,
            working_dir,
            config_dir,
            if resume { None } else { Some(prompt) },
            if resume {
                None
            } else {
                title.as_deref().map(sanitize_title)
            },
            attachments.unwrap_or_default(),
            resume,
            skip_permissions,
            sessions_arc,
        )
        .await;
        let _ = app.emit("session:debug", format!("[run_pragma] done: {sid_str}"));
    });

    Ok(session_id_str)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn send_control(
    app: AppHandle,
    session_id: String,
    message: String,
    binary: String,
    config_dir: Option<String>,
    working_dir: Option<String>,
    _model: Option<String>,
    sessions: tauri::State<'_, ActiveSessions>,
) -> Result<(), String> {
    let uuid = session_id
        .parse::<Uuid>()
        .map_err(|e| format!("session_id non valido: {e}"))?;

    let extra_args: Vec<String> = vec!["--resume".to_string(), session_id.clone(), message.clone()];

    let sessions_arc = Arc::clone(&sessions.0);

    tauri::async_runtime::spawn(async move {
        stream_claude(
            app,
            uuid,
            binary,
            extra_args,
            working_dir,
            config_dir,
            None,
            None,
            vec![],
            true,
            true,
            sessions_arc,
        )
        .await;
    });

    Ok(())
}

#[tauri::command]
async fn kill_session(
    session_id: String,
    sessions: tauri::State<'_, ActiveSessions>,
) -> Result<(), String> {
    let mut map = sessions.0.lock().await;
    if let Some(child) = map.get_mut(&session_id) {
        let _ = child.kill().await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Storage commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn list_sessions() -> Result<Vec<SessionInfo>, String> {
    list_saved_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_local_atom(atom: Atom) -> Result<(), String> {
    db_save_atom(&atom).map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_sessions(query: String) -> Result<Vec<SessionInfo>, String> {
    db_search_sessions(&query).map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_session(session_id: String) -> Result<Vec<Atom>, String> {
    load_session_atoms(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_session(session_id: String, title: Option<String>) -> Result<(), String> {
    mark_session_saved(&session_id).map_err(|e| e.to_string())?;
    if let Some(t) = title {
        let t = sanitize_title(&t);
        if !t.is_empty() {
            update_session_title(&session_id, &t).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn delete_session(session_id: String) -> Result<(), String> {
    db_delete_session(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn duplicate_session(session_id: String) -> Result<String, String> {
    db_duplicate_session(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_session(session_id: String) -> Result<String, String> {
    export_session_json(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_session_to_file(session_id: String) -> Result<String, String> {
    let settings = load_settings().map_err(|e| e.to_string())?;
    let dir = resolve_export_dir(&settings).map_err(|e| e.to_string())?;
    write_export_file(&session_id, &dir)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_session_to_markdown(session_id: String, content: String) -> Result<String, String> {
    let settings = load_settings().map_err(|e| e.to_string())?;
    let dir = resolve_export_dir(&settings).map_err(|e| e.to_string())?;
    write_markdown_file(&session_id, &content, &dir)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Settings commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_settings() -> Result<Settings, String> {
    load_settings_with_autodetect().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_settings_cmd(settings: Settings) -> Result<(), String> {
    save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
async fn detect_claude() -> Option<String> {
    detect_claude_binary()
}

// ---------------------------------------------------------------------------
// File/directory picker commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn pick_directory() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|f| f.path().to_string_lossy().into_owned())
}

#[tauri::command]
async fn pick_file() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .pick_file()
        .await
        .map(|f| f.path().to_string_lossy().into_owned())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(format!("URI scheme non consentito: {url}"));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url.as_str()])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn copy_to_pragmadocs(src: String, working_dir: String) -> Result<String, String> {
    do_copy_to_pragmadocs(&src, &working_dir)
}

#[tauri::command]
async fn save_session_attachments(
    session_id: String,
    filenames: Vec<String>,
) -> Result<(), String> {
    db_update_session_attachments(&session_id, &filenames).map_err(|e| e.to_string())
}

#[tauri::command]
async fn file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

// ---------------------------------------------------------------------------
// Trust store commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn trust_directory(dir: String, state: tauri::State<TrustedDirs>) -> Result<(), String> {
    state.insert(dir);
    Ok(())
}

#[tauri::command]
fn get_trusted_dirs(state: tauri::State<TrustedDirs>) -> Vec<String> {
    state.list()
}

#[tauri::command]
fn remove_trusted_dir(dir: String, state: tauri::State<TrustedDirs>) -> Result<(), String> {
    state.remove(&dir);
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .manage(ActiveSessions(Arc::new(TokioMutex::new(HashMap::new()))))
        .manage(TrustedDirs::load())
        .invoke_handler(tauri::generate_handler![
            run_pragma,
            send_control,
            kill_session,
            list_sessions,
            search_sessions,
            load_session,
            save_session,
            delete_session,
            duplicate_session,
            export_session,
            export_session_to_file,
            export_session_to_markdown,
            get_settings,
            save_settings_cmd,
            detect_claude,
            pick_directory,
            pick_file,
            copy_to_pragmadocs,
            save_session_attachments,
            file_exists,
            open_url,
            get_claude_usage,
            save_local_atom,
            trust_directory,
            get_trusted_dirs,
            remove_trusted_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

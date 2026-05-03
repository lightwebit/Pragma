use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::storage::pragma_db_path;

// ---------------------------------------------------------------------------
// OS detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    Windows,
    MacOs,
    Linux,
    Unknown,
}

impl HostOs {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown
        }
    }

    /// Cartella Downloads di default per l'OS corrente.
    pub fn default_export_dir() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)?;
        Some(home.join("Downloads"))
    }
}

// ---------------------------------------------------------------------------
// Strutture dati
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub label: String,
    pub binary: String,
    pub config_dir: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    /// Legacy — presente solo per migrazione. Non viene riscritto su JSON.
    #[serde(default, skip_serializing)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub profiles: Vec<Profile>,
    pub db_path: Option<String>,
    #[serde(default)]
    pub notifications_enabled: bool,
    #[serde(default)]
    pub default_profile_index: usize,
    /// Cartella di destinazione per export .md e .json.
    /// None = usa il default OS (~/Downloads).
    #[serde(default)]
    pub export_dir: Option<String>,
    #[serde(default)]
    pub diff_always_open: bool,
    /// Last working directory used — persisted across restarts.
    #[serde(default)]
    pub last_working_dir: Option<String>,
}

/// Risolve la cartella di export: usa quella configurata, altrimenti ~/Downloads.
/// Crea la cartella se non esiste.
pub fn resolve_export_dir(settings: &Settings) -> Result<PathBuf> {
    let dir = match &settings.export_dir {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => HostOs::default_export_dir()
            .unwrap_or_else(|| pragma_db_path().unwrap().parent().unwrap().join("exports")),
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// Path del file settings
// ---------------------------------------------------------------------------

pub fn settings_path() -> Result<PathBuf> {
    let db = pragma_db_path()?;
    Ok(db.parent().unwrap().join("settings.json"))
}

pub fn prompts_dir() -> Result<PathBuf> {
    let db = pragma_db_path()?;
    let dir = db.parent().unwrap().join("prompts");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Converte una label in nome file sicuro: lowercase, solo [a-z0-9_-].
fn label_to_filename(label: &str) -> String {
    label
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Carica il prompt per il profilo con la label data, se esiste.
/// NOTE: prompt encryption is not implemented. Prompts are stored as plain text in
///       ~/.pragma/prompts/<label>.txt — encryption is a planned post-v1 feature.
pub fn load_prompt(label: &str) -> Option<String> {
    let dir = prompts_dir().ok()?;
    let path = dir.join(format!("{}.txt", label_to_filename(label)));
    let text = std::fs::read_to_string(&path).ok()?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

// ---------------------------------------------------------------------------
// Lettura / scrittura
// ---------------------------------------------------------------------------

pub fn load_settings() -> Result<Settings> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("lettura settings: {}", path.display()))?;
    let mut settings: Settings = serde_json::from_str(&content).context("parsing settings.json")?;

    // Migrazione: system_prompt inline → file esterno
    let mut migrated = false;
    for profile in &mut settings.profiles {
        if let Some(prompt) = profile.system_prompt.take() {
            if !prompt.trim().is_empty() {
                if let Ok(dir) = prompts_dir() {
                    let file = dir.join(format!("{}.txt", label_to_filename(&profile.label)));
                    if !file.exists() {
                        let _ = std::fs::write(&file, &prompt);
                    }
                }
                migrated = true;
            }
        }
    }
    if migrated {
        // Riscrivi senza system_prompt (skip_serializing lo esclude già)
        let _ = save_settings(&settings);
    }

    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = settings_path()?;
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, json).with_context(|| format!("scrittura settings: {}", path.display()))
}

// ---------------------------------------------------------------------------
// Auto-detect binario claude
// ---------------------------------------------------------------------------

/// Cerca il binario claude nell'ordine:
/// 1. PATH di sistema (where/which)
/// 2. ~/.local/bin/claude  (installazione tipica Linux/Mac)
/// 3. %APPDATA%/../Local/Programs/claude/claude.exe (Windows)
pub fn detect_claude_binary() -> Option<String> {
    // 1. where/which
    let cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = std::process::Command::new(cmd).arg("claude").output() {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout);
            let first = path.lines().next().unwrap_or("").trim().to_string();
            if !first.is_empty() {
                return Some(first);
            }
        }
    }

    // 2. ~/.local/bin/claude
    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
    {
        let candidate = home.join(".local").join("bin").join("claude");
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
        // Windows: ~/.local/bin/claude.exe
        let candidate_exe = home.join(".local").join("bin").join("claude.exe");
        if candidate_exe.exists() {
            return Some(candidate_exe.to_string_lossy().to_string());
        }
    }

    None
}

/// Carica le settings e, se non ci sono profili, tenta di aggiungere un profilo
/// "claude" con il binario auto-detected. Restituisce le settings (modificate o no).
pub fn load_settings_with_autodetect() -> Result<Settings> {
    let mut settings = load_settings()?;
    if settings.profiles.is_empty() {
        if let Some(binary) = detect_claude_binary() {
            settings.profiles.push(Profile {
                label: "claude".to_string(),
                binary,
                config_dir: None,
                language: None,
                system_prompt: None,
            });
            save_settings(&settings)?;
        }
    }
    Ok(settings)
}

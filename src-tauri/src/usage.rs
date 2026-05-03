use pragma_parser::settings::detect_claude_binary;

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some(&'[') => {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(&']') => {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2 == '\x07' || c2 == '\u{9C}' {
                            break;
                        }
                        if c2 == '\\' {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_percent(text: &str) -> Option<u32> {
    let pos = text.find('%')?;
    let digits: String = text[..pos]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().ok()
}

fn parse_resets(text: &str) -> Option<String> {
    let pos = text.find("Resets").or_else(|| text.find("Reses"))?;
    let skip = if text[pos..].starts_with("Resets") {
        6
    } else {
        5
    };
    let after = text[pos + skip..].trim_start_matches('s').trim_start();
    let stop = after
        .find("Current")
        .or_else(|| after.find("Extra"))
        .or_else(|| after.find("What"))
        .or_else(|| after.find("Esc"))
        .unwrap_or(after.len().min(60));
    let s = after[..stop]
        .trim()
        .trim_end_matches('·')
        .trim()
        .to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn parse_spent(text: &str) -> Option<String> {
    let pos = text.find('$')?;
    let after = &text[pos..];
    let end = after
        .find("spent")
        .map(|e| e + 5)
        .unwrap_or(after.len().min(30));
    let s = after[..end].trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn extract_usage_sections(raw: &str) -> Vec<serde_json::Value> {
    let flat: String = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let section_defs: &[(&str, &str)] = &[
        ("Current session", "Current session"),
        ("Currentsession", "Current session"),
        ("Current week", "Current week"),
        ("Currentweek", "Current week"),
        ("Extra usage", "Extra usage"),
        ("Extrausage", "Extra usage"),
    ];

    let stop_markers = [
        "Current session",
        "Currentsession",
        "Current week",
        "Currentweek",
        "Extra usage",
        "Extrausage",
        "What's contributing",
        "What'scontributing",
        "Scanning",
        "Last 24h",
        "d to day",
    ];

    let mut sections: Vec<serde_json::Value> = Vec::new();
    let mut seen_titles: Vec<String> = Vec::new();

    for (raw_key, display_title) in section_defs {
        let Some(start) = flat.find(raw_key) else {
            continue;
        };
        if seen_titles.contains(&display_title.to_string()) {
            continue;
        }

        let body_start = start + raw_key.len();
        let body_end = stop_markers
            .iter()
            .filter_map(|m| flat[body_start..].find(m).map(|p| body_start + p))
            .min()
            .unwrap_or(flat.len());

        let body = &flat[body_start..body_end];
        let full = format!("{} {}", raw_key, body);

        let percent = parse_percent(&full);
        let resets = parse_resets(&full);
        let spent = if display_title.contains("Extra") {
            parse_spent(body)
        } else {
            None
        };

        sections.push(serde_json::json!({
            "title":   display_title,
            "percent": percent,
            "resets":  resets,
            "spent":   spent,
        }));
        seen_titles.push(display_title.to_string());
    }
    sections
}

#[tauri::command]
pub async fn get_claude_usage(
    binary: Option<String>,
    config_dir: Option<String>,
) -> Result<serde_json::Value, String> {
    let bin = binary
        .or_else(detect_claude_binary)
        .ok_or_else(|| "claude binary not found".to_string())?;

    let text = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::io::{Read, Write};
        use std::sync::mpsc;
        use std::time::Duration;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 50,
                cols: 220,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;

        let mut cmd = CommandBuilder::new(&bin);
        if let Some(ref cd) = config_dir {
            if !cd.is_empty() {
                cmd.env("CLAUDE_CONFIG_DIR", cd);
            }
        }
        let claude_dir = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| std::path::PathBuf::from(h).join(".claude"));
        if let Some(ref d) = claude_dir {
            if d.exists() {
                cmd.cwd(d);
            }
        }

        let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);

        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = tx.send(buf[..n].to_vec());
                    }
                }
            }
        });

        let mut startup_captured = Vec::<u8>::new();
        std::thread::sleep(Duration::from_millis(1500));
        while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(80)) {
            startup_captured.extend_from_slice(&chunk);
        }
        let startup_text = strip_ansi(&String::from_utf8_lossy(&startup_captured));
        let has_trust_prompt = startup_text.contains("trust")
            || startup_text.contains("Trust")
            || startup_text.contains("proceed")
            || startup_text.contains("Do you");
        if has_trust_prompt {
            writer.write_all(b"y\r").ok();
            std::thread::sleep(Duration::from_millis(800));
            while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(80)) {
                startup_captured.extend_from_slice(&chunk);
            }
        } else {
            writer.write_all(b"\r").ok();
            std::thread::sleep(Duration::from_millis(300));
        }
        std::thread::sleep(Duration::from_millis(2000));
        while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            startup_captured.extend_from_slice(&chunk);
        }

        writer.write_all(b"/usage\r").map_err(|e| e.to_string())?;

        let deadline = std::time::Instant::now() + Duration::from_millis(20000);
        let mut output: Vec<u8> = Vec::new();
        let mut idle_streak = 0u32;
        loop {
            if std::time::Instant::now() >= deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(chunk) => {
                    output.extend_from_slice(&chunk);
                    idle_streak = 0;
                    let preview = strip_ansi(&String::from_utf8_lossy(&output));
                    let has_extra =
                        preview.contains("Extra usage") || preview.contains("Extrausage");
                    let has_week =
                        preview.contains("Current week") || preview.contains("Currentweek");
                    if has_extra && has_week && preview.contains("Resets") {
                        std::thread::sleep(Duration::from_millis(600));
                        while let Ok(c) = rx.recv_timeout(Duration::from_millis(100)) {
                            output.extend_from_slice(&c);
                        }
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    idle_streak += 1;
                    let threshold = if output.is_empty() { 50 } else { 10 };
                    if idle_streak >= threshold {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        child.kill().ok();

        let usage_str = strip_ansi(&String::from_utf8_lossy(&output));
        let sections = extract_usage_sections(&usage_str);
        Ok(serde_json::json!({ "sections": sections }))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(text)
}

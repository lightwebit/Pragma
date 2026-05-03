use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use uuid::Uuid;

use pragma_parser::{
    parser::Parser,
    sink::{FanOutSink, StdoutSink},
    storage::{export_session_json, list_saved_sessions, mark_session_saved, SqliteSink},
    types::{AtomSink, OutputSource},
};

fn main() {
    #[cfg(windows)]
    set_console_utf8();

    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("run") => cmd_run(&args),
        Some("save") => cmd_save(&args),
        Some("history") => cmd_history(),
        Some("export") => cmd_export(&args),
        Some("--help") | Some("-h") | None => print_help(),
        Some(cmd) => {
            eprintln!("pragma: comando sconosciuto '{cmd}'");
            eprintln!("Usa 'pragma --help'.");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// pragma run -- <command> [args...]
// ---------------------------------------------------------------------------

fn cmd_run(args: &[String]) {
    let sep = args.iter().position(|a| a == "--");
    let cmd_args = match sep {
        Some(i) if i + 1 < args.len() => &args[i + 1..],
        _ => {
            eprintln!("Uso: pragma run -- <comando> [args...]");
            std::process::exit(1);
        }
    };

    let cmd = &cmd_args[0];
    let cmd_rest = &cmd_args[1..];
    let command_str = cmd_args.join(" ");

    let session_id = Uuid::new_v4();

    // SqliteSink — salva su ~/.pragma/pragma.db
    let sqlite = SqliteSink::new(session_id, Some(&command_str), None, None).unwrap_or_else(|e| {
        eprintln!("pragma: errore DB: {e}");
        std::process::exit(1);
    });

    // FanOut: NDJSON su stdout + SQLite
    let mut sink: Box<dyn AtomSink> = Box::new(FanOutSink::new(vec![
        Box::new(StdoutSink::new()),
        Box::new(sqlite),
    ]));

    let mut parser = Parser::new(session_id);

    let mut child = Command::new(cmd)
        .args(cmd_rest)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("pragma: impossibile avviare '{cmd}': {e}");
            std::process::exit(1);
        });

    let stdout = child.stdout.take().expect("stdout pipe mancante");
    let reader = BufReader::new(stdout);

    for raw_line in reader.split(b'\n') {
        match raw_line {
            Ok(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                let line = line.trim_end_matches('\r');
                for atom in parser.feed_line(line, OutputSource::Stdout) {
                    sink.push(atom).ok();
                }
            }
            Err(_) => break,
        }
    }

    for atom in parser.finalize(&OutputSource::Stdout) {
        sink.push(atom).ok();
    }
    sink.finalize().ok();

    let status = child.wait().unwrap_or_else(|e| {
        eprintln!("pragma: errore attesa processo: {e}");
        std::process::exit(1);
    });

    print_metrics(&parser, session_id);
    eprintln!(
        "  Usa 'pragma save {id}' per conservare questa sessione.",
        id = &session_id.to_string()[..8]
    );

    std::process::exit(status.code().unwrap_or(1));
}

// ---------------------------------------------------------------------------
// pragma save <session-id>
// ---------------------------------------------------------------------------

fn cmd_save(args: &[String]) {
    let id_prefix = match args.get(2) {
        Some(id) => id,
        None => {
            eprintln!("Uso: pragma save <session-id>");
            std::process::exit(1);
        }
    };

    match mark_session_saved(id_prefix) {
        Ok(full_id) => eprintln!("Sessione salvata: {full_id}"),
        Err(e) => {
            eprintln!("pragma save: {e}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// pragma history
// ---------------------------------------------------------------------------

fn cmd_history() {
    match list_saved_sessions() {
        Ok(sessions) if sessions.is_empty() => {
            eprintln!("Nessuna sessione salvata. Usa 'pragma save <id>' dopo una run.");
        }
        Ok(sessions) => {
            eprintln!("{:<38} {:<25} {:>5}  comando", "ID", "data", "atomi");
            eprintln!("{}", "─".repeat(80));
            for s in sessions {
                eprintln!(
                    "{:<38} {:<25} {:>5}  {}",
                    s.id,
                    &s.started_at[..19],
                    s.atom_count,
                    s.command.as_deref().unwrap_or("—"),
                );
            }
        }
        Err(e) => {
            eprintln!("pragma history: {e}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// pragma export <session-id>
// ---------------------------------------------------------------------------

fn cmd_export(args: &[String]) {
    let id_prefix = match args.get(2) {
        Some(id) => id,
        None => {
            eprintln!("Uso: pragma export <session-id>");
            std::process::exit(1);
        }
    };

    match export_session_json(id_prefix) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("pragma export: {e}");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn print_metrics(parser: &Parser, session_id: Uuid) {
    let m = parser.metrics();
    eprintln!("─────────────────────────────────────");
    eprintln!("  Pragma session summary");
    eprintln!("  session : {session_id}");
    eprintln!("  atoms   : {}", m.total);
    eprintln!("  ├─ file_touch : {}", m.file_touch);
    eprintln!("  ├─ diff       : {}", m.diff);
    eprintln!("  ├─ tool_use   : {}", m.tool_use);
    eprintln!("  ├─ error      : {}", m.error);
    eprintln!(
        "  └─ agent_note : {} ({:.1}%)",
        m.agent_note,
        m.agent_note_pct()
    );
    if m.agent_note_pct() > 40.0 {
        eprintln!();
        eprintln!(
            "  WARNING: High AGENT_NOTE rate ({:.1}%).",
            m.agent_note_pct()
        );
    }
    eprintln!("─────────────────────────────────────");
}

fn print_help() {
    eprintln!("Pragma — atomizzatore di output agenti AI");
    eprintln!();
    eprintln!("COMANDI:");
    eprintln!("  pragma run -- <comando> [args...]   Avvolge e atomizza in real-time");
    eprintln!("  pragma save <session-id>            Salva permanentemente una sessione");
    eprintln!("  pragma history                      Lista sessioni salvate");
    eprintln!("  pragma export <session-id>          Esporta sessione come JSON su stdout");
    eprintln!();
    eprintln!("ESEMPI:");
    eprintln!("  pragma run -- claude -p --dangerously-skip-permissions \"crea hello.rs\"");
    eprintln!("  pragma save b502a1cf");
    eprintln!("  pragma history");
}

#[cfg(windows)]
fn set_console_utf8() {
    use std::os::raw::c_uint;
    extern "system" {
        fn SetConsoleCP(wCodePageID: c_uint) -> i32;
        fn SetConsoleOutputCP(wCodePageID: c_uint) -> i32;
    }
    unsafe {
        SetConsoleCP(65001);
        SetConsoleOutputCP(65001);
    }
}

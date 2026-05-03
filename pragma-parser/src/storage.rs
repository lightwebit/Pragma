use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::types::{Atom, AtomSink};

// ---------------------------------------------------------------------------
// Costanti
// ---------------------------------------------------------------------------

/// Numero di atomi bufferizzati prima di un flush batch su SQLite.
const BATCH_SIZE: usize = 10;

// ---------------------------------------------------------------------------
// Schema SQL + Migration runner
// ---------------------------------------------------------------------------

const CURRENT_SCHEMA_VERSION: u32 = 2;

// Stato iniziale del DB (v0): tabelle e indici originali.
// Le colonne aggiunte in seguito e le tabelle FTS sono gestite dalle migrazioni.
const BASE_SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL DEFAULT 0);

CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    started_at  TEXT NOT NULL,
    command     TEXT,
    saved       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS atoms (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id),
    atom_type   TEXT NOT NULL,
    file_path   TEXT,
    file_type   TEXT,
    action      TEXT,
    content     TEXT NOT NULL,
    collapsed   INTEGER NOT NULL DEFAULT 0,
    source      TEXT NOT NULL,
    received_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_atoms_session  ON atoms(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_saved ON sessions(saved, started_at DESC);
";

fn get_schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
        r.get(0)
    })
    .or_else(|_| {
        conn.execute("INSERT INTO schema_version (version) VALUES (0)", [])?;
        Ok(0)
    })
}

fn set_schema_version(conn: &Connection, v: u32) -> Result<()> {
    conn.execute("UPDATE schema_version SET version = ?1", params![v])?;
    Ok(())
}

/// Applica in sequenza tutte le migrazioni mancanti. Idempotente.
fn run_migrations(conn: &Connection) -> Result<()> {
    let version = get_schema_version(conn)?;
    if version >= CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    if version < 1 {
        migrate_v1(conn)?;
        set_schema_version(conn, 1)?;
    }
    if version < 2 {
        migrate_v2(conn)?;
        set_schema_version(conn, 2)?;
    }

    Ok(())
}

/// v1 — aggiunge le colonne extra alla tabella sessions.
fn migrate_v1(conn: &Connection) -> Result<()> {
    for stmt in &[
        "ALTER TABLE sessions ADD COLUMN title TEXT",
        "ALTER TABLE sessions ADD COLUMN working_dir TEXT",
        "ALTER TABLE sessions ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN total_cost_usd REAL",
        "ALTER TABLE sessions ADD COLUMN attachments TEXT",
    ] {
        let _ = conn.execute(stmt, []); // no-op se la colonna esiste già
    }
    Ok(())
}

/// v2 — aggiunge la tabella FTS5, i trigger e fa il backfill iniziale.
fn migrate_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
            session_id UNINDEXED,
            body,
            tokenize='unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS fts_atoms_insert
        AFTER INSERT ON atoms BEGIN
            INSERT INTO sessions_fts(session_id, body) VALUES (NEW.session_id, NEW.content);
        END;

        CREATE TRIGGER IF NOT EXISTS fts_sessions_title
        AFTER UPDATE OF title ON sessions WHEN NEW.title IS NOT NULL BEGIN
            INSERT INTO sessions_fts(session_id, body) VALUES (NEW.id, NEW.title);
        END;
    ",
    )?;

    let fts_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions_fts", [], |r| r.get(0))
        .unwrap_or(0);
    if fts_count == 0 {
        conn.execute_batch(
            "INSERT INTO sessions_fts(session_id, body)
             SELECT session_id, content FROM atoms",
        )?;
    }
    Ok(())
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(BASE_SCHEMA)
        .context("inizializzazione schema base")?;
    run_migrations(conn)
}

// ---------------------------------------------------------------------------
// SqliteSink
// ---------------------------------------------------------------------------

pub struct SqliteSink {
    conn: Connection,
    session_id: Uuid,
    buffer: Vec<Atom>,
}

impl SqliteSink {
    /// Apre (o crea) `~/.pragma/pragma.db` e inizializza la sessione.
    pub fn new(
        session_id: Uuid,
        command: Option<&str>,
        title: Option<&str>,
        working_dir: Option<&str>,
    ) -> Result<Self> {
        let db_path = pragma_db_path()?;
        let conn = Connection::open(&db_path)
            .with_context(|| format!("apertura DB: {}", db_path.display()))?;
        init_db(&conn)?;

        conn.execute(
            "INSERT INTO sessions (id, started_at, command, title, working_dir, saved) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![
                session_id.to_string(),
                Utc::now().to_rfc3339(),
                command,
                title,
                working_dir,
            ],
        )
        .context("inserimento sessione")?;

        Ok(Self {
            conn,
            session_id,
            buffer: Vec::with_capacity(BATCH_SIZE),
        })
    }

    /// Apre il DB su una sessione già esistente (usato da send_control/resume).
    pub fn open_existing(session_id: Uuid) -> Result<Self> {
        let db_path = pragma_db_path()?;
        let conn = Connection::open(&db_path)
            .with_context(|| format!("apertura DB: {}", db_path.display()))?;
        init_db(&conn)?;
        Ok(Self {
            conn,
            session_id,
            buffer: Vec::with_capacity(BATCH_SIZE),
        })
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Marca la sessione come salvata permanentemente.
    pub fn save_session(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET saved = 1 WHERE id = ?1",
            params![self.session_id.to_string()],
        )?;
        Ok(())
    }

    fn flush_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        for atom in &self.buffer {
            tx.execute(
                "INSERT OR IGNORE INTO atoms
                 (id, session_id, atom_type, file_path, file_type, action,
                  content, collapsed, source, received_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    atom.id.to_string(),
                    atom.session_id.to_string(),
                    atom_type_str(&atom.atom_type),
                    atom.file_path,
                    atom.file_type.as_ref().map(file_type_str),
                    atom.action.as_ref().map(action_str),
                    atom.content,
                    atom.collapsed as i32,
                    source_str(&atom.source),
                    atom.received_at.to_rfc3339(),
                ],
            )?;
        }
        tx.commit()?;
        self.buffer.clear();
        Ok(())
    }
}

impl AtomSink for SqliteSink {
    fn push(&mut self, atom: Atom) -> Result<()> {
        self.buffer.push(atom);
        if self.buffer.len() >= BATCH_SIZE {
            self.flush_buffer()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.flush_buffer()
    }

    fn finalize(&mut self) -> Result<()> {
        self.flush_buffer()
        // La sessione rimane in DB con saved=0 (temporanea).
        // L'utente esegue `pragma save <id>` per renderla permanente.
    }
}

// ---------------------------------------------------------------------------
// Query pubbliche (usate da pragma-cli per i comandi save / history)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub started_at: String,
    pub command: Option<String>,
    pub title: Option<String>,
    pub working_dir: Option<String>,
    pub atom_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_cost_usd: Option<f64>,
    pub attachments: Vec<String>,
}

/// Lista tutte le sessioni (con atomi), dalla più recente.
pub fn list_saved_sessions() -> Result<Vec<SessionInfo>> {
    let db_path = pragma_db_path()?;
    if !db_path.exists() {
        return Ok(vec![]);
    }
    let conn = Connection::open(&db_path)?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.started_at, s.command, s.title, s.working_dir,
                COUNT(a.id) AS atom_count,
                s.input_tokens, s.output_tokens, s.cache_read_tokens, s.cache_write_tokens,
                s.total_cost_usd, s.attachments
         FROM sessions s
         LEFT JOIN atoms a ON a.session_id = s.id
         GROUP BY s.id
         HAVING COUNT(a.id) > 0
         ORDER BY s.started_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        let attachments_json: Option<String> = row.get(11)?;
        Ok(SessionInfo {
            id: row.get(0)?,
            started_at: row.get(1)?,
            command: row.get(2)?,
            title: row.get(3)?,
            working_dir: row.get(4)?,
            atom_count: row.get::<_, i64>(5)? as usize,
            input_tokens: row.get::<_, i64>(6)? as u64,
            output_tokens: row.get::<_, i64>(7)? as u64,
            cache_read_tokens: row.get::<_, i64>(8)? as u64,
            cache_write_tokens: row.get::<_, i64>(9)? as u64,
            total_cost_usd: row.get(10)?,
            attachments: attachments_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Carica tutti gli atomi di una sessione dal DB, ordinati per received_at.
pub fn load_session_atoms(session_id: &str) -> Result<Vec<Atom>> {
    use crate::types::{AtomType, FileAction, FileType, OutputSource};
    use chrono::DateTime;

    let db_path = pragma_db_path()?;
    if !db_path.exists() {
        return Ok(vec![]);
    }
    let conn = Connection::open(&db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, session_id, atom_type, file_path, file_type, action,
                content, collapsed, source, received_at
         FROM atoms WHERE session_id = ?1 ORDER BY received_at ASC",
    )?;

    let rows = stmt.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,         // id
            row.get::<_, String>(1)?,         // session_id
            row.get::<_, String>(2)?,         // atom_type
            row.get::<_, Option<String>>(3)?, // file_path
            row.get::<_, Option<String>>(4)?, // file_type
            row.get::<_, Option<String>>(5)?, // action
            row.get::<_, String>(6)?,         // content
            row.get::<_, i32>(7)?,            // collapsed
            row.get::<_, String>(8)?,         // source
            row.get::<_, String>(9)?,         // received_at
        ))
    })?;

    let mut atoms = Vec::new();
    for row in rows {
        let (
            id,
            sid,
            atom_type,
            file_path,
            file_type,
            action,
            content,
            collapsed,
            source,
            received_at,
        ) = row?;

        let atom_type = match atom_type.as_str() {
            "FileTouch" => AtomType::FileTouch,
            "Diff" => AtomType::Diff,
            "ToolUse" => AtomType::ToolUse,
            "Error" => AtomType::Error,
            "UserReply" => AtomType::UserReply,
            "PragmaEvent" => AtomType::PragmaEvent,
            _ => AtomType::AgentNote,
        };
        let file_type = file_type.as_deref().map(|s| match s {
            "Code" => FileType::Code,
            "Config" => FileType::Config,
            "Markup" => FileType::Markup,
            "Style" => FileType::Style,
            "Build" => FileType::Build,
            "Data" => FileType::Data,
            _ => FileType::Other,
        });
        let action = action.as_deref().map(|s| match s {
            "Create" => FileAction::Create,
            "Delete" => FileAction::Delete,
            _ => FileAction::Modify,
        });
        let source = if source == "Stderr" {
            OutputSource::Stderr
        } else {
            OutputSource::Stdout
        };
        let received_at = DateTime::parse_from_rfc3339(&received_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        atoms.push(Atom {
            id: id.parse().unwrap_or_else(|_| Uuid::new_v4()),
            session_id: sid.parse().unwrap_or_else(|_| Uuid::new_v4()),
            atom_type,
            file_path,
            file_type,
            action,
            content,
            collapsed: collapsed != 0,
            source,
            received_at,
        });
    }
    Ok(atoms)
}

/// Carica i metadati di una singola sessione.
pub fn load_session_info(session_id: &str) -> Result<SessionInfo> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.started_at, s.command, s.title, s.working_dir,
                COUNT(a.id) AS atom_count,
                s.input_tokens, s.output_tokens, s.cache_read_tokens, s.cache_write_tokens,
                s.total_cost_usd, s.attachments
         FROM sessions s
         LEFT JOIN atoms a ON a.session_id = s.id
         WHERE s.id = ?1
         GROUP BY s.id",
    )?;
    stmt.query_row(params![session_id], |row| {
        let attachments_json: Option<String> = row.get(11)?;
        Ok(SessionInfo {
            id: row.get(0)?,
            started_at: row.get(1)?,
            command: row.get(2)?,
            title: row.get(3)?,
            working_dir: row.get(4)?,
            atom_count: row.get::<_, i64>(5)? as usize,
            input_tokens: row.get::<_, i64>(6)? as u64,
            output_tokens: row.get::<_, i64>(7)? as u64,
            cache_read_tokens: row.get::<_, i64>(8)? as u64,
            cache_write_tokens: row.get::<_, i64>(9)? as u64,
            total_cost_usd: row.get(10)?,
            attachments: attachments_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
        })
    })
    .map_err(Into::into)
}

#[derive(serde::Serialize)]
struct SessionExport<'a> {
    meta: SessionInfo,
    atoms: &'a [crate::types::Atom],
}

/// Esporta la sessione come JSON con metadati + atomi.
pub fn export_session_json(session_id: &str) -> Result<String> {
    let meta = load_session_info(session_id)?;
    let atoms = load_session_atoms(session_id)?;
    let export = SessionExport {
        meta,
        atoms: &atoms,
    };
    serde_json::to_string_pretty(&export).map_err(Into::into)
}

/// Scrive il JSON della sessione nella cartella indicata e restituisce il path.
pub fn write_export_file(
    session_id: &str,
    export_dir: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let json = export_session_json(session_id)?;
    std::fs::create_dir_all(export_dir)?;
    let short = &session_id[..session_id.len().min(8)];
    let path = export_dir.join(format!("pragma-session-{short}.json"));
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Scrive il Markdown (generato lato frontend) nella cartella indicata e restituisce il path.
pub fn write_markdown_file(
    session_id: &str,
    content: &str,
    export_dir: &std::path::Path,
) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(export_dir)?;
    let short = &session_id[..session_id.len().min(8)];
    let path = export_dir.join(format!("pragma-session-{short}.md"));
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Aggiorna il titolo di una sessione dato il suo ID (o prefisso).
pub fn update_session_title(id_prefix: &str, title: &str) -> Result<()> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;
    conn.execute(
        "UPDATE sessions SET title = ?1 WHERE id LIKE ?2 || '%'",
        params![title, id_prefix],
    )?;
    Ok(())
}

/// Aggiorna i contatori token/costo di una sessione (sovrascrive, i valori Claude sono cumulativi).
pub fn update_session_usage(
    session_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    total_cost_usd: Option<f64>,
) -> Result<()> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;
    conn.execute(
        "UPDATE sessions SET input_tokens=?1, output_tokens=?2, cache_read_tokens=?3, cache_write_tokens=?4, total_cost_usd=?5 WHERE id=?6",
        params![input_tokens as i64, output_tokens as i64, cache_read_tokens as i64, cache_write_tokens as i64, total_cost_usd, session_id],
    )?;
    Ok(())
}

/// Aggiorna la lista degli allegati di una sessione (JSON array di nomi file).
pub fn update_session_attachments(session_id: &str, filenames: &[String]) -> Result<()> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;
    let json = serde_json::to_string(filenames)?;
    conn.execute(
        "UPDATE sessions SET attachments = ?1 WHERE id = ?2",
        params![json, session_id],
    )?;
    Ok(())
}

/// Duplica una sessione (nuovi UUID per sessione e atomi), titolo con " (copy)".
/// Restituisce il nuovo session_id.
#[allow(clippy::type_complexity)]
pub fn duplicate_session(source_id: &str) -> Result<String> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;

    let (command, title, working_dir): (Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT command, title, working_dir FROM sessions WHERE id = ?1",
            params![source_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

    let new_id = Uuid::new_v4();
    let new_title = title.as_deref().unwrap_or("").to_string() + " (copy)";
    conn.execute(
        "INSERT INTO sessions (id, started_at, command, title, working_dir, saved)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        params![
            new_id.to_string(),
            Utc::now().to_rfc3339(),
            command,
            new_title,
            working_dir,
        ],
    )?;

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare(
        "SELECT atom_type, file_path, file_type, action, content, collapsed, source, received_at
         FROM atoms WHERE session_id = ?1 ORDER BY received_at ASC",
    )?;
    let rows: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i32,
        String,
        String,
    )> = stmt
        .query_map(params![source_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    drop(stmt);

    for (atom_type, file_path, file_type, action, content, collapsed, source, received_at) in rows {
        tx.execute(
            "INSERT INTO atoms
             (id, session_id, atom_type, file_path, file_type, action,
              content, collapsed, source, received_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                Uuid::new_v4().to_string(),
                new_id.to_string(),
                atom_type,
                file_path,
                file_type,
                action,
                content,
                collapsed,
                source,
                received_at,
            ],
        )?;
    }
    tx.commit()?;

    Ok(new_id.to_string())
}

/// Elimina una sessione e tutti i suoi atomi dal DB.
pub fn delete_session(session_id: &str) -> Result<()> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;
    conn.execute(
        "DELETE FROM atoms WHERE session_id = ?1",
        params![session_id],
    )?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
    Ok(())
}

/// Inserisce un singolo atomo direttamente nel DB (usato per UserReply/PragmaEvent dal frontend).
pub fn save_atom(atom: &Atom) -> Result<()> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;
    conn.execute(
        "INSERT OR IGNORE INTO atoms
         (id, session_id, atom_type, file_path, file_type, action,
          content, collapsed, source, received_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            atom.id.to_string(),
            atom.session_id.to_string(),
            atom_type_str(&atom.atom_type),
            atom.file_path,
            atom.file_type.as_ref().map(file_type_str),
            atom.action.as_ref().map(action_str),
            atom.content,
            atom.collapsed as i32,
            source_str(&atom.source),
            atom.received_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Marca una sessione come salvata dato il suo ID (o prefisso).
pub fn mark_session_saved(id_prefix: &str) -> Result<String> {
    let db_path = pragma_db_path()?;
    let conn = Connection::open(&db_path)?;

    // Supporta prefisso breve (es. prime 8 cifre dell'UUID)
    let full_id: String = conn
        .query_row(
            "SELECT id FROM sessions WHERE id LIKE ?1 || '%' LIMIT 1",
            params![id_prefix],
            |row| row.get(0),
        )
        .context("sessione non trovata — controlla l'ID")?;

    conn.execute(
        "UPDATE sessions SET saved = 1 WHERE id = ?1",
        params![full_id],
    )?;
    Ok(full_id)
}

// ---------------------------------------------------------------------------
// Full-text search
// ---------------------------------------------------------------------------

/// Converte la query utente in una query FTS5 safe con prefix matching per token.
fn sanitize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ricerca full-text nelle sessioni salvate (titolo + contenuto atomi).
pub fn search_sessions(query: &str) -> Result<Vec<SessionInfo>> {
    if query.trim().is_empty() {
        return list_saved_sessions();
    }
    let db_path = pragma_db_path()?;
    if !db_path.exists() {
        return Ok(vec![]);
    }
    let conn = Connection::open(&db_path)?;
    init_db(&conn)?;

    let safe_query = sanitize_fts_query(query);
    let mut stmt = conn.prepare(
        "SELECT s.id, s.started_at, s.command, s.title, s.working_dir,
                COUNT(a.id) AS atom_count,
                s.input_tokens, s.output_tokens, s.cache_read_tokens, s.cache_write_tokens,
                s.total_cost_usd, s.attachments
         FROM (SELECT DISTINCT session_id FROM sessions_fts WHERE sessions_fts MATCH ?1) f
         JOIN sessions s ON s.id = f.session_id
         LEFT JOIN atoms a ON a.session_id = s.id
         GROUP BY s.id
         ORDER BY s.started_at DESC",
    )?;
    let rows = stmt.query_map(params![safe_query], |row| {
        let attachments_json: Option<String> = row.get(11)?;
        Ok(SessionInfo {
            id: row.get(0)?,
            started_at: row.get(1)?,
            command: row.get(2)?,
            title: row.get(3)?,
            working_dir: row.get(4)?,
            atom_count: row.get::<_, i64>(5)? as usize,
            input_tokens: row.get::<_, i64>(6)? as u64,
            output_tokens: row.get::<_, i64>(7)? as u64,
            cache_read_tokens: row.get::<_, i64>(8)? as u64,
            cache_write_tokens: row.get::<_, i64>(9)? as u64,
            total_cost_usd: row.get(10)?,
            attachments: attachments_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Helpers di serializzazione
// ---------------------------------------------------------------------------

fn atom_type_str(t: &crate::types::AtomType) -> &'static str {
    use crate::types::AtomType::*;
    match t {
        FileTouch => "FileTouch",
        Diff => "Diff",
        ToolUse => "ToolUse",
        Error => "Error",
        AgentNote => "AgentNote",
        UserReply => "UserReply",
        PragmaEvent => "PragmaEvent",
    }
}

fn file_type_str(t: &crate::types::FileType) -> &'static str {
    use crate::types::FileType::*;
    match t {
        Code => "Code",
        Config => "Config",
        Markup => "Markup",
        Style => "Style",
        Build => "Build",
        Data => "Data",
        Other => "Other",
    }
}

fn action_str(a: &crate::types::FileAction) -> &'static str {
    use crate::types::FileAction::*;
    match a {
        Create => "Create",
        Modify => "Modify",
        Delete => "Delete",
    }
}

fn source_str(s: &crate::types::OutputSource) -> &'static str {
    use crate::types::OutputSource::*;
    match s {
        Stdout => "Stdout",
        Stderr => "Stderr",
    }
}

// ---------------------------------------------------------------------------
// Path del DB + sicurezza permessi
// ---------------------------------------------------------------------------

/// Rimuove l'ereditarietà dei permessi sulla directory e concede accesso
/// esclusivo all'utente corrente. Non usa /T: i file esistenti mantengono i
/// propri ACL (ereditati dal parent), e i nuovi file erediteranno l'ACE OI+CI
/// applicato alla directory stessa.
/// Best-effort: gli errori vengono ignorati silenziosamente.
#[cfg(windows)]
fn restrict_to_owner(path: &std::path::Path) {
    let Some(path_str) = path.to_str() else {
        return;
    };
    let username = std::env::var("USERNAME").unwrap_or_default();
    if username.is_empty() {
        return;
    }
    let _ = std::process::Command::new("icacls")
        .args([
            path_str,
            "/inheritance:r",
            "/grant:r",
            &format!("{}:(OI)(CI)F", username),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(unix)]
fn restrict_to_owner(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(any(windows, unix)))]
fn restrict_to_owner(_path: &std::path::Path) {}

/// Assicura che i permessi della directory siano impostati al massimo una
/// volta per lifetime del processo (OnceLock).
static PRAGMA_DIR_SECURED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub fn pragma_db_path() -> Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .context("HOME directory non trovata")?;
    let dir = home.join(".pragma");
    std::fs::create_dir_all(&dir)?;
    PRAGMA_DIR_SECURED.get_or_init(|| restrict_to_owner(&dir));
    Ok(dir.join("pragma.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn schema_init_sets_current_version() {
        let conn = in_memory_db();
        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn session_round_trip() {
        let conn = in_memory_db();
        let id = uuid::Uuid::new_v4();
        conn.execute(
            "INSERT INTO sessions (id, started_at, command, title, working_dir, saved) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![id.to_string(), "2024-01-01T00:00:00Z", None::<String>, Some("My Session"), None::<String>],
        ).unwrap();

        let title: Option<String> = conn
            .query_row(
                "SELECT title FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, Some("My Session".to_string()));

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn atom_insert_and_query() {
        let conn = in_memory_db();
        let session_id = uuid::Uuid::new_v4();
        let atom_id = uuid::Uuid::new_v4();

        conn.execute(
            "INSERT INTO sessions (id, started_at, command, saved) VALUES (?1, ?2, ?3, 0)",
            params![
                session_id.to_string(),
                "2024-01-01T00:00:00Z",
                None::<String>
            ],
        )
        .unwrap();

        conn.execute(
            "INSERT OR IGNORE INTO atoms \
             (id, session_id, atom_type, content, collapsed, source, received_at) \
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                atom_id.to_string(),
                session_id.to_string(),
                "AgentNote",
                "hello world",
                "stdout",
                "2024-01-01T00:00:00Z",
            ],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM atoms WHERE session_id = ?1",
                params![session_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Duplicate insert must be a no-op (INSERT OR IGNORE)
        conn.execute(
            "INSERT OR IGNORE INTO atoms \
             (id, session_id, atom_type, content, collapsed, source, received_at) \
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            params![
                atom_id.to_string(),
                session_id.to_string(),
                "AgentNote",
                "duplicate",
                "stdout",
                "2024-01-01T00:00:00Z",
            ],
        )
        .unwrap();
        let count2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM atoms WHERE session_id = ?1",
                params![session_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count2, 1);
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = in_memory_db();
        // Running migrations again must not fail
        run_migrations(&conn).unwrap();
        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }
}

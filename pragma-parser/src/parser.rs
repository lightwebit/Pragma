use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::types::{detect_file_type, Atom, AtomType, FileAction, OutputSource};

// ---------------------------------------------------------------------------
// Session-level metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ParseMetrics {
    pub total: usize,
    pub file_touch: usize,
    pub diff: usize,
    pub tool_use: usize,
    pub error: usize,
    pub agent_note: usize,
}

impl ParseMetrics {
    pub fn agent_note_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.agent_note as f64 / self.total as f64 * 100.0
    }
}

// ---------------------------------------------------------------------------
// Parser — consumes NDJSON from `claude --output-format stream-json --verbose`
// ---------------------------------------------------------------------------

pub struct Parser {
    session_id: Uuid,
    pub metrics: ParseMetrics,
}

impl Parser {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            metrics: ParseMetrics::default(),
        }
    }

    /// Feed one NDJSON line. Returns zero or more atoms.
    pub fn feed_line(&mut self, line: &str, _source: OutputSource) -> Vec<Atom> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }

        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return vec![], // ignore non-JSON lines
        };

        let atoms = match v["type"].as_str() {
            Some("assistant") => self.handle_assistant(&v),
            Some("user") => self.handle_user(&v),
            Some("result") => self.handle_result(&v),
            _ => vec![], // system, rate_limit_event, etc.
        };

        for atom in &atoms {
            self.metrics.total += 1;
            match atom.atom_type {
                AtomType::FileTouch => self.metrics.file_touch += 1,
                AtomType::Diff => self.metrics.diff += 1,
                AtomType::ToolUse => self.metrics.tool_use += 1,
                AtomType::Error => self.metrics.error += 1,
                AtomType::AgentNote => self.metrics.agent_note += 1,
                AtomType::UserReply | AtomType::PragmaEvent => {}
            }
        }

        atoms
    }

    /// No multi-line state in the JSON parser — nothing to flush.
    pub fn finalize(&mut self, _source: &OutputSource) -> Vec<Atom> {
        vec![]
    }

    pub fn metrics(&self) -> &ParseMetrics {
        &self.metrics
    }

    // -----------------------------------------------------------------------
    // Event handlers
    // -----------------------------------------------------------------------

    fn handle_assistant(&self, v: &Value) -> Vec<Atom> {
        let contents = match v["message"]["content"].as_array() {
            Some(arr) => arr,
            None => return vec![],
        };

        let mut atoms = Vec::new();
        for item in contents {
            match item["type"].as_str() {
                Some("tool_use") => {
                    let name = item["name"].as_str().unwrap_or("unknown");
                    let input_str =
                        serde_json::to_string_pretty(&item["input"]).unwrap_or_default();
                    // First line = tool name (used as preview in AtomCard)
                    let content = format!("{name}\n{input_str}");
                    atoms.push(self.make_atom(
                        AtomType::ToolUse,
                        None,
                        None,
                        None,
                        content,
                        true,
                        OutputSource::Stdout,
                    ));
                }
                Some("text") => {
                    let text = item["text"].as_str().unwrap_or("").trim().to_string();
                    if !text.is_empty() {
                        atoms.push(self.make_atom(
                            AtomType::AgentNote,
                            None,
                            None,
                            None,
                            text,
                            true,
                            OutputSource::Stdout,
                        ));
                    }
                }
                _ => {} // thinking, etc. → ignore
            }
        }
        atoms
    }

    fn handle_user(&self, v: &Value) -> Vec<Atom> {
        let tur = &v["tool_use_result"];
        if !tur.is_object() {
            return vec![];
        }

        let file_path = tur["filePath"].as_str().map(|s| s.to_string());
        let mut atoms = Vec::new();

        // FileTouch
        let action = match tur["type"].as_str() {
            Some("create") => Some(FileAction::Create),
            Some("modify") => Some(FileAction::Modify),
            Some("delete") => Some(FileAction::Delete),
            _ => None,
        };

        if let Some(action) = action {
            let file_type = file_path.as_deref().map(detect_file_type);
            let content = file_path.clone().unwrap_or_default();
            atoms.push(self.make_atom(
                AtomType::FileTouch,
                file_path.clone(),
                file_type,
                Some(action),
                content,
                false,
                OutputSource::Stdout,
            ));
        }

        // Diff — only if structuredPatch is non-empty
        if let Some(patch) = tur["structuredPatch"].as_array() {
            if !patch.is_empty() {
                let file_type = file_path.as_deref().map(detect_file_type);
                let content = serde_json::to_string_pretty(patch).unwrap_or_default();
                atoms.push(self.make_atom(
                    AtomType::Diff,
                    file_path.clone(),
                    file_type,
                    None,
                    content,
                    true,
                    OutputSource::Stdout,
                ));
            }
        }

        atoms
    }

    fn handle_result(&self, v: &Value) -> Vec<Atom> {
        if v["is_error"].as_bool().unwrap_or(false) {
            let content = v["result"].as_str().unwrap_or("error").to_string();
            return vec![self.make_atom(
                AtomType::Error,
                None,
                None,
                None,
                content,
                false,
                OutputSource::Stderr,
            )];
        }
        vec![]
    }

    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn make_atom(
        &self,
        atom_type: AtomType,
        file_path: Option<String>,
        file_type: Option<crate::types::FileType>,
        action: Option<FileAction>,
        content: String,
        collapsed: bool,
        source: OutputSource,
    ) -> Atom {
        Atom {
            id: Uuid::new_v4(),
            atom_type,
            file_path,
            file_type,
            action,
            content,
            collapsed,
            source,
            received_at: Utc::now(),
            session_id: self.session_id,
        }
    }
}

use pragma_parser::types::{Atom, AtomType};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Pragma protocol — injected as --append-system-prompt on every run.
// ---------------------------------------------------------------------------

pub const PRAGMA_PROTOCOL: &str = r#"
You are operating within the Pragma protocol. You MUST structure every response using the exact markers below — no exceptions, no deviations.

== FLOW ==

1. Always start with:
   ANALYSIS:
   <concise analysis of the request — what is being asked and any relevant constraints>

2. If the task is ambiguous or requires clarification before proceeding, emit:
   QUESTIONS:
   - <question 1>
   - <question 2>
   AWAITING_ANSWERS
   Then STOP and wait for the user's reply before continuing.

3. Once the task is clear, emit the execution plan:
   PLAN:
   1. <step 1>
   2. <step 2>
   ...
   AWAITING_CONFIRMATION
   Then STOP. Do not execute anything until the user confirms.

4. After confirmation, execute each step in order. After completing each step emit:
   STEP_COMPLETE: <N>
   <brief description of what was done>

5. After all steps are complete, emit:
   REPORT:
   <summary of completed work, files created/modified, key decisions>

6. Always emit a verification checklist:
   TESTS:
   - <verification step 1>
   - <verification step 2>
   - <verification step 3>

7. Finally, always close with:
   CLOSING:
   <one short sentence — confirm completion and invite follow-up questions>
   AWAITING_CLOSE

== RULES ==
- Use these markers exactly as written (uppercase, colon after section names).
- IMPORTANT: Always write the marker keywords in English (ANALYSIS:, QUESTIONS:, PLAN:, etc.) regardless of the language you use for content. Only the content inside each section may be in the user's language.
- Never skip AWAITING_CONFIRMATION — always pause after PLAN and wait for approval.
- Never skip TESTS — always include at least 2 verification items.
- Never skip CLOSING / AWAITING_CLOSE — always end every response with them.
- Do not add markdown headers (##) or bold (**) around the markers themselves.
- Keep section content concise and actionable.
- THIS IS NON-OPTIONAL: even for short, conversational, or evaluative prompts (opinions, quick answers, explanations), you MUST still use the markers. There is NO scenario where you may respond with plain prose or markdown headers instead.
- Minimum flow for non-task responses (no plan needed): ANALYSIS: → CLOSING: → AWAITING_CLOSE
- NEVER use ## headers, **bold labels**, or any markdown structure as a substitute for the markers above.
"#;

/// Step-by-step addendum — appended when the user requires manual approval between steps.
pub const PRAGMA_STEP_BY_STEP: &str = r#"
== STEP-BY-STEP MODE (MANDATORY) ==
CRITICAL CONSTRAINT — you MUST obey this without exception:
After every STEP_COMPLETE: <N>, if there are more steps remaining, you MUST emit:
AWAITING_CONFIRMATION
Then STOP COMPLETELY. Do not write another word. Do not begin the next step.
You may only continue after receiving CONTINUE from the user.
Ignoring this constraint will cause the session to abort.
"#;

// ---------------------------------------------------------------------------
// Pragma protocol state machine — per-session
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum SectionKind {
    None,
    Analysis,
    Questions,
    Plan,
    StepComplete(u32),
    Report,
    Tests,
    Closing,
}

/// Per-session state machine for pragma protocol parsing.
///
/// Persists across successive atoms to handle:
/// - Streaming deltas: section opened in one atom, content in the next
/// - Cumulative streaming: deduplication to avoid re-emitting identical payloads
/// - End of session: explicit flush of any still-open section
pub struct PragmaEmitState {
    section: SectionKind,
    buf: Vec<String>,
    found_any_marker: bool,
    last_emitted: std::collections::HashMap<&'static str, String>,
}

impl PragmaEmitState {
    pub fn new() -> Self {
        Self {
            section: SectionKind::None,
            buf: Vec::new(),
            found_any_marker: false,
            last_emitted: std::collections::HashMap::new(),
        }
    }

    pub fn process_atom(&mut self, app: &AppHandle, atom: &Atom) {
        if atom.atom_type != AtomType::AgentNote {
            return;
        }

        let normalized = normalize_pragma_content(&atom.content);

        let first_meaningful = normalized
            .lines()
            .map(|l| strip_markdown_prefix(l.trim_end()))
            .find(|l| !l.is_empty())
            .unwrap_or("");
        if is_section_header_start(first_meaningful) || is_phase_keyword(first_meaningful) {
            self.section = SectionKind::None;
            self.buf.clear();
        }

        for raw in normalized.lines() {
            let line = raw.trim_end();
            let norm = strip_markdown_prefix(line);

            if let Some((kind, inline)) = detect_section_header(norm) {
                self.flush_current(app);
                self.section = kind;
                self.buf.clear();
                self.found_any_marker = true;
                if let Some(text) = inline {
                    if !text.is_empty() {
                        if matches!(self.section, SectionKind::Plan) {
                            self.buf.extend(split_numbered_inline(text));
                        } else {
                            self.buf.push(text.to_string());
                        }
                    }
                }
            } else if let Some(phase) = detect_phase(norm) {
                self.flush_current(app);
                self.section = SectionKind::None;
                self.buf.clear();
                self.found_any_marker = true;
                self.emit_dedup(
                    app,
                    "pragma:phase",
                    serde_json::Value::String(phase.to_string()),
                );
            } else {
                self.buf.push(line.to_string());
            }
        }
    }

    pub fn flush_pending(&mut self, app: &AppHandle, fallback_content: Option<&str>) {
        self.flush_current(app);

        if !self.found_any_marker {
            if let Some(text) = fallback_content {
                let trimmed = text.trim();
                if trimmed.len() > 20 {
                    self.emit_dedup(
                        app,
                        "pragma:raw_note",
                        serde_json::json!({ "text": trimmed }),
                    );
                }
            }
        }

        self.section = SectionKind::None;
        self.buf.clear();
        self.found_any_marker = false;
    }

    fn flush_current(&mut self, app: &AppHandle) {
        if matches!(self.section, SectionKind::None) {
            return;
        }
        let payload = build_section_payload(&self.section, &self.buf);
        let event = section_event_name(&self.section);
        if let (Some(payload), Some(event)) = (payload, event) {
            self.emit_dedup(app, event, payload);
        }
    }

    fn emit_dedup(&mut self, app: &AppHandle, event: &'static str, payload: serde_json::Value) {
        let serialized = payload.to_string();
        if self
            .last_emitted
            .get(event)
            .map(|s| s == &serialized)
            .unwrap_or(false)
        {
            return;
        }
        self.last_emitted.insert(event, serialized);
        let _ = app.emit(event, payload);
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

fn normalize_pragma_content(text: &str) -> String {
    const STARTERS: &[&str] = &[
        "ANALYSIS:",
        "QUESTIONS:",
        "PLAN:",
        "STEP_COMPLETE:",
        "REPORT:",
        "TESTS:",
        "CLOSING:",
        "AWAITING_ANSWERS",
        "AWAITING_APPROVAL",
        "AWAITING_CONFIRMATION",
        "AWAITING_CLOSE",
    ];
    let mut out = text.to_string();
    for kw in STARTERS {
        out = out.replace(&format!(" {kw}"), &format!("\n{kw}"));
    }
    out
}

fn strip_markdown_prefix(line: &str) -> &str {
    let s = line.trim_start_matches('#').trim();
    let s = s.trim_start_matches("**").trim_end_matches("**").trim();
    s.trim_start_matches('*').trim()
}

fn is_section_header_start(norm: &str) -> bool {
    matches!(
        norm,
        "ANALYSIS:" | "QUESTIONS:" | "PLAN:" | "REPORT:" | "TESTS:" | "CLOSING:"
    ) || norm.starts_with("ANALYSIS: ")
        || norm.starts_with("QUESTIONS: ")
        || norm.starts_with("PLAN: ")
        || norm.starts_with("REPORT: ")
        || norm.starts_with("TESTS: ")
        || norm.starts_with("CLOSING: ")
        || norm.starts_with("STEP_COMPLETE: ")
}

fn is_phase_keyword(norm: &str) -> bool {
    matches!(
        norm,
        "AWAITING_ANSWERS" | "AWAITING_APPROVAL" | "AWAITING_CLOSE"
    ) || norm.starts_with("AWAITING_CONFIRMATION")
}

fn detect_section_header(norm: &str) -> Option<(SectionKind, Option<&str>)> {
    macro_rules! section {
        ($kw:literal, $kind:expr) => {
            if norm == $kw {
                return Some(($kind, None));
            }
            if let Some(inline) = norm.strip_prefix(concat!($kw, " ")) {
                return Some(($kind, Some(inline)));
            }
        };
    }
    section!("ANALYSIS:", SectionKind::Analysis);
    section!("QUESTIONS:", SectionKind::Questions);
    section!("PLAN:", SectionKind::Plan);
    section!("REPORT:", SectionKind::Report);
    section!("TESTS:", SectionKind::Tests);
    section!("CLOSING:", SectionKind::Closing);

    if let Some(rest) = norm.strip_prefix("STEP_COMPLETE: ") {
        let mut parts = rest.splitn(2, ' ');
        let n = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        let inline = parts.next();
        return Some((SectionKind::StepComplete(n), inline));
    }
    None
}

fn detect_phase(norm: &str) -> Option<&'static str> {
    match norm {
        "AWAITING_ANSWERS" => Some("awaiting_answers"),
        "AWAITING_APPROVAL" => Some("awaiting_approval"),
        "AWAITING_CLOSE" => Some("awaiting_close"),
        _ if norm.starts_with("AWAITING_CONFIRMATION") => Some("awaiting_confirmation"),
        _ => None,
    }
}

fn build_section_payload(sec: &SectionKind, lines: &[String]) -> Option<serde_json::Value> {
    let text = lines.join("\n");
    let text = text.trim().to_string();
    match sec {
        SectionKind::Analysis | SectionKind::Report | SectionKind::Closing => {
            if text.is_empty() {
                return None;
            }
            Some(serde_json::json!({ "text": text }))
        }
        SectionKind::Questions | SectionKind::Tests => {
            let items: Vec<String> = lines
                .iter()
                .map(|l| l.trim_start_matches("- ").trim().to_string())
                .filter(|s| !s.is_empty() && !s.chars().all(|c| c == '-'))
                .collect();
            if items.is_empty() {
                return None;
            }
            Some(serde_json::json!({ "items": items }))
        }
        SectionKind::Plan => {
            let steps: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
            if steps.is_empty() {
                return None;
            }
            Some(serde_json::json!({ "steps": steps }))
        }
        SectionKind::StepComplete(n) => {
            if text.is_empty() {
                return None;
            }
            Some(serde_json::json!({ "step": n, "result": text }))
        }
        SectionKind::None => None,
    }
}

fn section_event_name(sec: &SectionKind) -> Option<&'static str> {
    match sec {
        SectionKind::Analysis => Some("pragma:analysis"),
        SectionKind::Questions => Some("pragma:questions"),
        SectionKind::Plan => Some("pragma:plan"),
        SectionKind::StepComplete(_) => Some("pragma:step_complete"),
        SectionKind::Report => Some("pragma:report"),
        SectionKind::Tests => Some("pragma:tests"),
        SectionKind::Closing => Some("pragma:closing"),
        SectionKind::None => None,
    }
}

fn split_numbered_inline(text: &str) -> Vec<String> {
    let mut steps: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut n: u32 = 2;
    let mut remaining = text;
    loop {
        let needle = format!(" {n}. ");
        if let Some(pos) = remaining.find(&needle) {
            current.push_str(&remaining[..pos]);
            if !current.trim().is_empty() {
                steps.push(current.trim().to_string());
            }
            current = format!("{n}. ");
            remaining = &remaining[pos + needle.len()..];
            n += 1;
        } else {
            current.push_str(remaining);
            if !current.trim().is_empty() {
                steps.push(current.trim().to_string());
            }
            break;
        }
    }
    if steps.is_empty() && !text.trim().is_empty() {
        steps.push(text.trim().to_string());
    }
    steps
}

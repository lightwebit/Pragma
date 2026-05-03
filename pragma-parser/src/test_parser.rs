// Metti questo file in pragma-parser/src/ e aggiungi al Cargo.toml:
// [[bin]]
// name = "test-parser"
// path = "src/test_parser.rs"
//
// Poi: cargo run --bin test-parser

fn main() {
    use pragma_parser::parser::Parser;
    use pragma_parser::types::OutputSource;
    use uuid::Uuid;

    let mut parser = Parser::new(Uuid::new_v4());

    // Righe NDJSON reali da `claude --output-format stream-json --verbose`
    let test_lines = vec![
        r#"{"type":"system","subtype":"init","session_id":"abc123"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Write","input":{"file_path":"src/main.rs","content":"fn main() {}"}}]}}"#,
        r#"{"type":"user","tool_use_result":{"type":"create","filePath":"src/main.rs","content":"fn main() {}","structuredPatch":[]}}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Ho creato il file src/main.rs."}]}}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls -la"}}]}}"#,
        r#"{"type":"result","subtype":"success","is_error":false,"result":"Fatto."}"#,
        r#"{"type":"result","subtype":"error","is_error":true,"result":"Permesso negato."}"#,
    ];

    for line in &test_lines {
        let atoms = parser.feed_line(line, OutputSource::Stdout);
        if atoms.is_empty() {
            println!("NO ATOM  | {}", &line[..line.len().min(80)]);
        } else {
            for atom in &atoms {
                println!(
                    "{:?} | path={:?} action={:?} | {}",
                    atom.atom_type,
                    atom.file_path,
                    atom.action,
                    &atom.content[..atom.content.len().min(60)]
                );
            }
        }
    }
}

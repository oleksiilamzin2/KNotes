use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use knotes_index::SqliteIndex;
use knotes_indexer::Indexer;
use knotes_md::MarkdownParser;
use knotes_vault::FsVault;
use serde_json::{Value, json};

/// A vault with two notes, indexed where Knotes keeps its index under `home`.
fn vault_and_home() -> (tempfile::TempDir, tempfile::TempDir) {
    let (vault, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(vault.path().join("Radar.md"), "# Radar\n\nThe radar project starts in May. [[Boris]]").unwrap();
    std::fs::write(vault.path().join("Boris.md"), "# Boris\n\nWorks on [[Radar]].").unwrap();
    let dir = knotes_session::index_dir(&home.path().join("Library/Application Support/Knotes"), vault.path());
    std::fs::create_dir_all(&dir).unwrap();
    let mut index = SqliteIndex::open(&dir).unwrap();
    Indexer::new(FsVault::new(vault.path()), MarkdownParser::new()).sync(&mut index).unwrap();
    (vault, home)
}

/// Sends `requests` (one JSON-RPC message each) to the server and gives its answers.
fn talk(vault: &std::path::Path, home: &std::path::Path, requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes-mcp")).arg(vault).env("HOME", home).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let answers = BufReader::new(child.stdout.take().unwrap()).lines().map(|l| serde_json::from_str(&l.unwrap()).unwrap()).collect();
    assert!(child.wait().unwrap().success());
    answers
}

#[test]
fn the_server_introduces_itself_and_lists_the_vault_tools() {
    let (vault, home) = vault_and_home();
    let answers = talk(
        vault.path(),
        home.path(),
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        ],
    );
    assert_eq!(answers.len(), 2, "no answer to a notification: {answers:?}");
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "knotes");
    assert_eq!(answers[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(answers[0]["result"]["capabilities"]["tools"].is_object());
    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "search" && t["inputSchema"]["type"] == "object"), "{tools:?}");
}

#[test]
fn a_tool_call_answers_with_its_result_as_text_and_an_error_is_flagged() {
    let (vault, home) = vault_and_home();
    let answers = talk(
        vault.path(),
        home.path(),
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "links", "arguments": {"path": "Radar.md"}}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "read_note", "arguments": {"path": "Nope.md"}}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "no/such/method"}),
        ],
    );
    let text = answers[0]["result"]["content"][0]["text"].as_str().unwrap();
    let result: Value = serde_json::from_str(text).unwrap();
    assert_eq!(result["backlinks"][0]["path"], "Boris.md", "{text}");
    assert_eq!(answers[0]["result"]["isError"], false);
    assert_eq!(answers[1]["result"]["isError"], true);
    assert!(answers[1]["result"]["content"][0]["text"].as_str().unwrap().contains("Nope.md"));
    assert_eq!(answers[2]["error"]["code"], -32601);
}

#[test]
fn a_vault_knotes_has_not_indexed_is_said_plainly() {
    let (vault, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let out = Command::new(env!("CARGO_BIN_EXE_knotes-mcp")).arg(vault.path()).env("HOME", home.path()).stdin(Stdio::null()).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("open the vault in Knotes first"), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn without_a_vault_named_the_last_one_knotes_opened_is_served() {
    let (vault, home) = vault_and_home();
    let settings = knotes_session::AppSettings { last_vault: Some(vault.path().to_path_buf()), ..Default::default() };
    settings.save(&home.path().join("Library/Application Support/Knotes")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_knotes-mcp")).env("HOME", home.path()).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    writeln!(child.stdin.take().unwrap(), "{}", json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "find_by_title", "arguments": {"query": "bor"}}})).unwrap();
    let out = child.wait_with_output().unwrap();
    let answer: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(answer["id"], 7);
    assert!(answer["result"]["content"][0]["text"].as_str().unwrap().contains("Boris.md"), "{answer}");
}

//! `knotes-mcp [vault]`: an MCP server (stdio) with the read-only tools over a vault Knotes
//! has indexed (ADR 0027); without a vault, the last one Knotes opened.

mod server;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use knotes_index::SqliteIndex;
use knotes_pdf::PdfTexts;
use knotes_session::{AppSettings, app_data_dir, index_dir};
use knotes_tools::VaultTools;
use knotes_vault::FsVault;

use crate::server::McpServer;

fn main() -> ExitCode {
    let app_data = app_data_dir();
    let Some(vault) = std::env::args_os().nth(1).map(PathBuf::from).or_else(|| AppSettings::load(&app_data).last_vault) else {
        eprintln!("knotes-mcp: name a vault: knotes-mcp <vault folder>");
        return ExitCode::FAILURE;
    };
    let index = match SqliteIndex::open_reader(&index_dir(&app_data, &vault)) {
        Ok(index) => index,
        Err(e) => {
            eprintln!("knotes-mcp: no index of {} ({e}); open the vault in Knotes first", vault.display());
            return ExitCode::FAILURE;
        }
    };
    let server = McpServer::new(VaultTools::new(index, FsVault::new(&vault), PdfTexts::new(&vault)));
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = server.answer_line(&line)
            && writeln!(out, "{answer}").and_then(|()| out.flush()).is_err()
        {
            break;
        }
    }
    ExitCode::SUCCESS
}

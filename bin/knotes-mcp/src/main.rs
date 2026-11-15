//! `knotes-mcp [vault]`: an MCP server (stdio) with the read-only tools over a vault Knotes
//! has indexed (ADR 0027); without a vault, the last one Knotes opened.

mod server;

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use knotes_index::SqliteIndex;
use knotes_pdf::PdfTexts;
use knotes_embed::ProcessEmbedder;
use knotes_session::{AppSettings, app_data_dir, embedder_files, index_dir};
use knotes_vectors::{MeaningIndex, MeaningModel, VectorStore};
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
    let mut tools = VaultTools::new(index, FsVault::new(&vault), PdfTexts::new(&vault));
    // Search by meaning over the vectors Knotes keeps (ADR 0028), with a helper of its own.
    // The model chosen in Knotes' settings (ADR 0029); none when the user turned it off.
    if let Some(model) = MeaningModel::from_setting(AppSettings::load(&app_data).meaning_model.as_deref())
        && let (Some((helper, model)), Ok(store)) = (std::env::current_exe().ok().and_then(|exe| embedder_files(&exe, model.file())), VectorStore::open_reader(&index_dir(&app_data, &vault), &model.store_file()))
    {
        tools = tools.with_meaning(Box::new(MeaningIndex::over_store(Arc::new(ProcessEmbedder::new(helper, model)), store)));
    }
    let server = McpServer::new(tools);
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

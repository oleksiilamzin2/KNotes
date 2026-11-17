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
        && let (Some((helper, file)), Ok(store)) = (std::env::current_exe().ok().and_then(|exe| embedder_files(&exe, model.file())), VectorStore::open_reader(&index_dir(&app_data, &vault), &model.store_file()))
    {
        tools = tools.with_meaning(Box::new(MeaningIndex::over_store(Arc::new(ProcessEmbedder::new(helper, file)), store)), model.closeness());
    }
    tools.set_utc_offset(utc_offset());
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

/// How far this Mac's clock is ahead of UTC, in seconds, as `date +%z` says it (`+0200`); 0
/// when it does not: a deck's day then begins at UTC's midnight.
fn utc_offset() -> i64 {
    let Ok(out) = std::process::Command::new("/bin/date").arg("+%z").output() else { return 0 };
    parse_offset(String::from_utf8_lossy(&out.stdout).trim()).unwrap_or(0)
}

fn parse_offset(zone: &str) -> Option<i64> {
    let (sign, digits) = (if zone.starts_with('-') { -1 } else { 1 }, zone.trim_start_matches(['+', '-']));
    (digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| sign * (digits[..2].parse::<i64>().unwrap_or(0) * 3600 + digits[2..].parse::<i64>().unwrap_or(0) * 60))
}

#[cfg(test)]
mod tests {
    use super::parse_offset;

    #[test]
    fn the_clock_s_offset_is_read_as_date_prints_it() {
        assert_eq!((parse_offset("+0200"), parse_offset("-0330"), parse_offset("+0000")), (Some(7200), Some(-12_600), Some(0)));
        assert_eq!((parse_offset(""), parse_offset("EET"), parse_offset("+02")), (None, None, None));
    }
}

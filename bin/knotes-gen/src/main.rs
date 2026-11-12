//! `knotes-gen <out-dir> [--notes N] [--seed S]` writes a synthetic vault into an empty folder.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use knotes_synth::{SyntheticVault, VaultSpec};

#[derive(Debug, PartialEq)]
struct Args {
    out: PathBuf,
    notes: usize,
    seed: u64,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let (mut out, mut notes, mut seed) = (None, 10_000, 1);
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--notes" => notes = value("--notes")?.parse().map_err(|e| format!("--notes: {e}"))?,
            "--seed" => seed = value("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            path if out.is_none() => out = Some(PathBuf::from(path)),
            extra => return Err(format!("unexpected argument {extra}")),
        }
    }
    Ok(Args { out: out.ok_or("missing <out-dir>")?, notes, seed })
}

/// Refuses to write into a folder that has anything in it, so a real vault is never overwritten.
fn ensure_empty(dir: &Path) -> Result<(), String> {
    match std::fs::read_dir(dir).map(|mut entries| entries.next().is_some()) {
        Ok(true) => Err(format!("{} is not empty", dir.display())),
        _ => Ok(()),
    }
}

fn main() -> ExitCode {
    let run = || -> Result<(), String> {
        let args = parse_args(std::env::args().skip(1))?;
        ensure_empty(&args.out)?;
        let start = Instant::now();
        let vault = SyntheticVault::plan(VaultSpec::new(args.notes, args.seed));
        let planned = start.elapsed();
        let bytes = vault.write_to(&args.out).map_err(|e| e.to_string())?;
        println!(
            "{} notes, {:.1} MB in {} (plan {:.2?}, total {:.2?})",
            vault.len(),
            bytes as f64 / 1e6,
            args.out.display(),
            planned,
            start.elapsed()
        );
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("knotes-gen: {e}\nusage: knotes-gen <out-dir> [--notes N] [--seed S]");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse_args(s.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn parses_out_dir_and_flags() {
        assert_eq!(args("v --notes 50 --seed 3"), Ok(Args { out: "v".into(), notes: 50, seed: 3 }));
        assert_eq!(args("v"), Ok(Args { out: "v".into(), notes: 10_000, seed: 1 }));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(args("").is_err());
        assert!(args("v --notes").is_err());
        assert!(args("v --notes x").is_err());
        assert!(args("v --bogus 1").is_err());
        assert!(args("v w").is_err());
    }

    #[test]
    fn only_empty_or_missing_folders_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ensure_empty(dir.path()).is_ok());
        assert!(ensure_empty(&dir.path().join("missing")).is_ok());
        std::fs::write(dir.path().join("note.md"), "x").unwrap();
        assert!(ensure_empty(dir.path()).is_err());
    }
}

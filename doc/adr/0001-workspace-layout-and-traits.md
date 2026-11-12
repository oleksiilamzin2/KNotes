# 0001. Workspace layout and trait boundaries

Status: accepted, 2026-11-12

## Context

Knotes is a native macOS knowledge base over a folder of Markdown and canvas files,
written in Rust. Code priorities, in order: fast, then extensible, then compact.
The owner asked for a codebase deeply split into traits, one trait per file, and
binaries and libraries kept in separate `bin/` and `lib/` folders.

## Decision

- One Cargo workspace. Library crates live in `lib/<crate>`, binaries in `bin/<crate>`.
- `lib/knotes-core` owns the domain model (parsed note, links, tags, headings, blocks)
  and the traits (ports) the rest of the system talks through. It has no I/O and no
  heavy dependencies.
- Every adapter (Markdown parser, SQLite store, search engines, file watcher, AppKit UI,
  chat client, MCP server) is a separate crate that implements core traits.
- Each trait sits in its own file; `lib.rs` only declares modules and re-exports.
- Hot paths use static dispatch (generics). `dyn Trait` is allowed only where the call
  rate is low (UI wiring, settings, tool registry) or where plugins need it.
- New crates are added when a micro-task needs them, not ahead of time.
- Generated test vaults go to `target/vaults/` and are never committed.

## Consequences

- Adapters can be replaced and tested in isolation; the core stays fast to compile.
- More crates means more `Cargo.toml` boilerplate; that cost is accepted.
- Static dispatch keeps search and indexing free of virtual calls, at the price of
  some generic noise in signatures.

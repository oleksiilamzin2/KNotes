# 0028. Search by meaning: EmbeddingGemma through llama.cpp in a helper process

Status: accepted, 2026-11-14

## Context

The search the chat and MCP agents use is FTS5 over blocks with looser fallbacks (ADR 0027).
On the retrieval benchmark (`lib/knotes-tools/tests/home`) queries by words find 36 of 37,
queries by meaning 0 of 9: a note that says the same in other words, or in the other
language, is never found. The research in `doc/research/knotes-ai-improvements` found every
serious notes harness keeps a semantic layer beside the lexical one, and EmbeddingGemma the
multilingual embedding model that fits the memory budget. The owner chose to build it at once,
with llama.cpp inside the app in a process of its own, the model in the bundle, fetched by the
development setup script (2026-11-14).

Checked on 2026-11-14:

- `ggml-org/embeddinggemma-300M-GGUF` on Hugging Face is not gated and holds one file,
  `embeddinggemma-300M-Q8_0.gguf`, 333 590 944 bytes, SHA-256 `b5ce9d77…0490d63`. The model
  is under the Gemma Terms of Use, which allow passing it on with their notice.
- The model card's prompts: queries `task: search result | query: {text}`, documents
  `title: {title or "none"} | text: {text}`. Its vectors are 768 wide and may be cut to 512,
  256 or 128 (Matryoshka); 256 loses about 1.5 points of MTEB Multilingual.
- `llama-cpp-2` (crates.io) builds llama.cpp with cmake; its `metal` feature runs it on the GPU.
  The machine had no cmake.

## Decision

- **Model:** EmbeddingGemma-300M, Q8_0 GGUF, in `Knotes.app/Contents/Resources`.
  `scripts/setup.sh` installs the build tools (cmake, through Homebrew) and downloads the model
  into `models/` (not kept in git), checking its SHA-256; `scripts/bundle.sh` copies it in.
- **Runtime:** llama.cpp through `llama-cpp-2` with Metal, linked statically into a helper
  binary, `knotes-embed`, beside `knotes` and `knotes-mcp` in `Contents/MacOS`.
- **A process of its own:** a crash in C++ never takes the editor down with unsaved text, the
  model's memory goes back when the helper exits after a few idle minutes, and hours of first
  indexing run at background priority. The app and the MCP server start it when they need it
  and again when it has gone; they talk to it over stdin and stdout, a JSON line each way.
- **Vectors:** the first 256 of 768, normalized, as `i8` (×127): 256 bytes a block, about
  51 MB for 200 000 blocks. Kept in the index's SQLite file with a hash of the text embedded,
  so only blocks that changed are embedded again; searched by brute force in memory.
- **What a block is embedded as:** `title: {note title} | text: {heading path} › {block}`.
- **Search:** meaning runs beside the block search, not as one more fallback (the fallbacks run
  only when too little is found, and a query by meaning often finds enough of the wrong
  thing), and the two lists are always merged, so exact matches keep their place: reciprocal
  rank fusion (k = 60) with the first lexical hit kept first. Finds by meaning are marked
  (`match: meaning`) for the model to read before it cites them. No separate tool.
- **Indexing:** in the background, changed blocks only, a batch at a time; the table is its own
  progress, so a stop loses nothing.
- **Accepted when:** by meaning well above 0 of 9 on a larger set, and by words not below the
  benchmark's floor. The set by meaning grows to 40–60 queries first.

## Consequences

- The bundle grows by about 340 MB; building needs cmake; the first build of llama.cpp is slow.
- The helper's cold start (loading the model) comes before the first search by meaning after
  idle; to be measured, with throughput and memory on an M1.
- The Gemma Terms of Use notice has to ship with the app.
- Core ML (the neural engine, cheaper on battery) can take llama.cpp's place behind the same
  trait if first indexing proves too heavy.

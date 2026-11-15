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

## Built (2026-11-14)

- `scripts/setup.sh` installs cmake through Homebrew and fetches the model into `models/`,
  checking its SHA-256; `scripts/bundle.sh` refuses to build without it, puts `knotes-embed`
  beside `knotes` and `knotes-mcp` and the model in Resources, and signs all three (the bundle
  is 391 MB in debug).
- `knotes-embed` (library): `Passage` (a query or a block, put in the model card's prompts),
  `Vector` (the first 256 of 768, normalized, ×127 in `i8`), the `Embedder` trait, the helper's
  line protocol and `ProcessEmbedder`, which starts the helper at the first request and again
  when it has gone (once idle, or dead), with its stderr sent nowhere: llama.cpp's Metal logs
  cannot be silenced, and errors come back in the protocol.
- `knotes-llama`: `LlamaEmbedder` over `llama-cpp-2` 0.1.161 with Metal, every layer on the GPU,
  up to 32 passages and 2048 tokens a decode. A context is made for each request: a context
  kept for the process (the model leaked to give it a `'static` borrow) made Metal abort at exit,
  finding buffers still held. `llama-cpp-2` asserts the model file is there in debug builds, so
  the embedder checks first. `knotes-embed-helper` is the helper binary, `knotes-embed`: a JSON
  line in, a JSON line out, gone after five idle minutes.
- Measured on the development machine, an Apple M5 Pro (not the M1 the targets are set for,
  whose GPU is several times smaller; to be measured there): loading the model 3.1 s the first
  time (2.7 s of it compiling Metal's shaders), 0.2 s once macOS keeps them; about 420 blocks a
  second (2.4 ms a block); a query 10 ms.
- `NoteBlocks` (`knotes-core`, from the index's `blocks` table) gives a document's search blocks
  as full-text search holds them, so vectors and full-text finds name the same blocks.
  `MeaningSearch` is the search the tools take.
- `knotes-vectors`: what a block is embedded as (the document's file name as title; its section's
  heading before its text unless the text starts with it; the heading alone, not its whole path,
  for now), a `Snapshot` searched by brute force, `VectorStore` and `update`. The vectors live in
  `vectors.sqlite` beside the index rather than in it: written in the background, they never
  wait on the indexer's lock. A row a document: its date when embedded, its blocks' places (JSON)
  and their vectors; a store made another way (`EMBEDDING`) is emptied. `update` drops documents
  gone and embeds those new or changed since, the newest first, 32 passages a request, stopping
  between batches when told. `MeaningIndex` searches the store, read again when SQLite's
  `data_version` says another connection wrote, looked at once every ten seconds at most.
- The tools merge finds by meaning into search's by reciprocal rank fusion (k = 60), the first
  find by words kept first; a block found both ways keeps how words found it, one found only by
  meaning is `match: meaning`; documents kept from the AI are left out of both. A failing search
  by meaning leaves search by words as it was.
- The app: one `ProcessEmbedder` for the chat's tools and a thread that keeps the vault's
  vectors up to its index, at once and every 15 s, pausing 20 ms after each batch for the
  window's sake; another vault stops it. The probe never starts the model. The MCP server
  searches the vectors Knotes keeps with a helper of its own; it does not embed.
- The benchmark runs twice, by words alone and with meaning merged: by words 36 → 37 of 37
  (MRR 0.81 → 0.87), by meaning 1 → 6 of 9 (MRR 0.06 → 0.50), all 37 → 43 of 46. Still missed:
  "термін дії" and two wordings of October's electricity (the readings are dated `2026-10-01`).
  Its floors now hold the run with meaning too.
- In the app on the household vault: its 58 documents (100 blocks) embedded within seconds of
  opening; through the MCP server "скільки світла спожили в жовтні" finds the meter readings,
  "чим годувати кота" the cat's note, "vehicle oil change" the car's service log.

## Built (2026-11-14): when the vectors are made

- Documents gone are found through a set: comparing each stored path with every indexed one
  was 2.5 billion comparisons a pass on 50 000 notes.
- Only blocks whose text changed go to the model: the store keeps, with each block's place, a
  hash (FNV-1a) of the text it was embedded as, its title included, and a changed document's
  unchanged blocks keep their vectors. Editing one paragraph of a long note embeds one block.
- A document changed within the last 15 s (`SETTLE`; a minute at first, shortened at the
  owner's word) is being written (saved every two seconds): the background leaves it, and the
  pass says when the first such document settles. One under active editing — changed again
  within five minutes (`EDITING`) of the version last embedded — waits a minute
  (`SETTLE_EDITING`), at the owner's word: told from the files' dates alone, so edits made in
  another editor count too.
- Before each turn of the chat, the documents still settling are embedded at once (`Which::
  Waiting`), unpaced, so the model's search finds what the user has just written; the backlog of
  first indexing stays the background's, never kept waiting for. Search checks the store's
  version at each query (no longer once in ten seconds), so it sees them.
- No polling: the window tells the chat worker each time the index changes (each snapshot the
  session sends), and the thread keeping the vectors sleeps until then, or until a document
  left for being written settles, and lets a run of changes end (2 s without one) before a
  pass. While nothing changes it never wakes. Another vault drops its waker, which ends it.

Still to do: a set of 40–60 queries by meaning before calling the quality settled; the M1's
numbers, and whether hours of first indexing on a large vault keep the window smooth; heading
paths; the Gemma Terms of Use notice in the app.

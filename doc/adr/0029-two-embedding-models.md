# 0029. Two embedding models to choose in the settings: e5-large-instruct and EmbeddingGemma 2

Status: accepted, 2026-11-15. Supersedes ADR 0028's choice of model (EmbeddingGemma-300M);
the rest of ADR 0028 stands.

## Context

ADR 0028 built search by meaning on EmbeddingGemma-300M. Measured on the benchmark's 65 queries
by meaning (ADR 0028, "Measured (2026-11-15)"), it found 54 of 65 (MRR 0.72) at 256 dimensions;
tuning around it gave little, and Qwen3-Embedding-0.6B and Nemotron-3-Embed-1B did worse. A web
search for newer small multilingual models (2026-11-15) named EmbeddingGemma 2 (Google, September
2026, Apache 2.0, a 310 MB Q8_0 GGUF from ggml-org), multilingual-e5-large-instruct (Microsoft,
MIT, 603 MB) and BGE-M3 (BAAI, MIT, 635 MB). Measured the same way (the tools' search with
meaning merged in; the list's search):

| Model | Dimensions | By meaning | MRR |
|---|---|---|---|
| EmbeddingGemma-300M | 256 | 0.83 | 0.72 |
| EmbeddingGemma 2 | 256 / 768 | 0.92 / 0.94 | 0.76 / 0.78 |
| multilingual-e5-large-instruct | 256 / 512 / 1024 | 0.91 / 0.97 / 0.98 | 0.76 / 0.80 / 0.80 |
| BGE-M3 | 256 / 512 / 1024 | 0.82 / 0.91 / 0.95 | 0.71 / 0.78 / 0.81 |

On an M5 Pro EmbeddingGemma-300M embedded 780 blocks a second, EmbeddingGemma 2 480, e5 430,
BGE-M3 460; a query took 15–23 ms with each. EmbeddingGemma 2 needs a llama.cpp newer than the
one `llama-cpp-2` 0.1.161 (crates.io, 2026-11-03) bundles: its architecture, `gemma-embedding2`,
came in llama.cpp's PR #30054 (2026-10-06); llama-cpp-rs's `main` carries llama.cpp of
2026-11-04 since 2026-11-11 (commit d94c594), not yet released. The owner asked for a choice
between e5 and EmbeddingGemma 2 in the settings, on a «Пошук» tab, with llama-cpp-2 taken from
git until it is released, both models in the bundle.

## Decision

- **Models:** multilingual-e5-large-instruct (the default: 63 of 65) and EmbeddingGemma 2
  (lighter: 310 MB against 603), both Q8_0 GGUF in `Contents/Resources`, fetched by
  `scripts/setup.sh` with their SHA-256s. EmbeddingGemma-300M and the Gemma Terms of Use go:
  EmbeddingGemma 2 is under Apache 2.0, e5 under MIT; `licenses/` carries both licenses and
  llama.cpp's.
- **llama.cpp:** `llama-cpp-2` from git, llama-cpp-rs commit d94c594, until a crates.io release
  carries it. Built and measured on that commit; EmbeddingGemma-300M scored the same on it.
- **Prompts:** EmbeddingGemma 2 takes EmbeddingGemma's; e5 an English instruction before a
  query (`Instruct: …\nQuery: …`) and a document as it is. The helper tells the model from its
  name, or its file's when the file names none. e5's window is 512 tokens (its training), the
  helper takes the smaller of 2048 and the model's.
- **Vectors 512 wide for both** (e5 63 of 65 at 512, 64 at 1024; twice the memory of 256:
  about 100 MB for 200 000 blocks). One `Vector` type, so no model needs its own.
- **A store a model:** `vectors-<key>.sqlite` beside the index, so going back to a model already
  embedded with is instant. `MeaningModel` (knotes-vectors) names each model's key, file,
  store, way of embedding and the list's thresholds.
- **The list's thresholds a model**, tuned on the benchmark (each scores on a scale of its own;
  the old ones listed 31 notes a query with e5): e5 0.70 and 0.03 behind the nearest (all 63
  found, 7.0 notes a query; 0.025 lost one), EmbeddingGemma 2 0.30 and 0.05 (62 found, 6.3 a
  query; 0.04 lost one).
- **The settings:** Knotes → Налаштування… (Cmd+,) opens a window of tabs, «Пошук» for now:
  «Пошук за змістом:» e5-large-instruct, EmbeddingGemma 2 or Вимкнено, kept as `meaningModel`
  in `settings-v2.json` (none: the default; `off`). Choosing replaces the app's one helper; the
  chat worker restarts the thread keeping the vectors with the new model and rebuilds its tools
  at the next turn; the list's search remakes its search by meaning when the model it was made
  for is no longer the one chosen; off greys the switch's «За змістом» segment (a search by
  meaning under way goes back to file names). The MCP server reads the same setting.
- **Stored vectors in the settings** (at the owner's word): the «Пошук» tab shows, for each
  model, how much its stores take in all vaults (with SQLite's journal) and one action: the
  model not in use is cleared (its files removed, room freed); the one in use is made again
  («Перегенерувати»: its files removed, the windows told, the thread keeping the vectors starts
  from nothing). Under them a line and a bar say how far the vectors being made have gone and how
  fast ("Генерація векторів: 1 240 з 5 000 нотаток · 380 блоків/с"), from what each pass reports
  (`update_reporting`, at its start and after each batch); "Вектори актуальні" otherwise. The
  window looks again twice a second while shown. A generation, raised by each model chosen and
  each store cleared, tells the list's search to open its store again.
- **Long blocks embedded in pieces** (2026-11-15), whole lines of about 400 characters, a
  table's rows with its header row, each piece its own place in the store; search gives a block
  once, as its nearest piece (whose text the list's row and the tools' result then show). With
  EmbeddingGemma-300M pieces had not helped (ADR 0028); with these models, measured against whole
  blocks (by meaning, MRR in the tools' search / the list's): e5 0.81 → 0.83 / 0.83 → 0.86,
  EmbeddingGemma 2 0.78 → 0.82 / 0.81 → 0.86, nothing lost, the list's noise about the same;
  pieces of 240 and 320 lost answers with EmbeddingGemma 2, of 600 with e5. The way of embedding
  is v2 for both, so every store is embedded again.
- **The list shows little for what the vault does not hold** (2026-11-15, the owner: queries
  that find nothing listed a heap of unrelated notes). The benchmark got 14 queries the vault
  has no answer to (sushi, a sole trader's tax return, bitcoin…), counted apart: with cosine
  thresholds the list showed 11.8 notes for each with e5 and 15.3 with EmbeddingGemma 2, more
  than for queries it answers, since unrelated notes crowd together (e5 scores them 0.7–0.8).
  A find by meaning is now scored by how far it stands out from all the blocks searched, in
  standard deviations, and the list's thresholds are in those: e5 1.8 and 1.6 behind the
  nearest, EmbeddingGemma 2 1.7 and 1.5, which list 3.9 and 4.9 notes for queries with no
  answer, 3.9 and 3.7 for those answered (7.4 and 6.3 before), finding as much. Question and
  function words left out of the words searched now include prepositions, conjunctions,
  particles and pronouns ("за" found every word starting with it). The benchmark holds a ceiling
  on what the list shows for queries with no answer.
- **The tools' search keeps the same thresholds** (2026-11-15): it gave every find by meaning
  beside those by words, so the chat's model and MCP clients got 8.8 (e5) and 9.2
  (EmbeddingGemma 2) documents for queries with no answer, unrelated notes to read or to build an
  answer on. With the list's thresholds a model they get 3.9 and 4.9, finding as much (by
  meaning 63 and 62 of 65; EmbeddingGemma 2's MRR 0.82 → 0.81); looser thresholds found no more
  and gave 5.7–9.2. Each block found by meaning says how far it stands out (`closeness`, one
  decimal), so the model can tell a match from a neighbour: on the benchmark 50–59% of the
  answers' blocks stood at 3 or more, 7–17% of the rest; below 2.5 mostly the rest. The benchmark
  holds the same ceiling on the tools' search.
- **`related` gives the notes near in meaning** (2026-11-15): it gave links only, so a note no
  link joins was connected with nothing. Now, with search by meaning, also up to 10 notes near
  the document (the mean of its blocks' vectors against the other documents' blocks, each note
  once as its nearest block, with `closeness`), those its links show and those kept from the AI
  left out (`near_in_meaning`). One note to another stands out more than a query to a block:
  the query's thresholds gave 4.5 notes a note on the benchmark's vault (a car beside a washing
  machine), so documents take their own, 2.2 and 1.2 behind the nearest, for both models: 2.6
  notes a note with e5, 2.7 with EmbeddingGemma 2, by reading mostly about the same thing (no
  benchmark measures it yet).

- **The chat measured with it** (2026-11-15, GPT-5.6 Terra at «Довгі роздуми», e5): the 30
  questions of ADR 0027 answered as on 2026-11-14 (the same three partly: a disagreement between
  a note and a contract, a missing passport, one medicine of three), in 90 tool calls instead of
  111 and 389 s instead of 466 (the to-dos 28 → 13 calls), 14 % more tokens (the effort was not
  recorded before, so the two cannot be told apart). Eleven new questions, all answered: six in
  other words than the notes' (one in English), a trap whose answer is "no" (no comprehensive
  car insurance) and four the vault has no answer to, none made up. Those four cost the most:
  5, 9, 22 and 26 tool calls (up to 26 000 tokens and 47 s), the model searching on before
  saying the vault lacks it.

## Consequences

- The bundle is about 945 MB (debug), the two models 913 MB of it.
- A git dependency until llama-cpp-2 0.1.162: to be replaced by the release (and checked against
  the benchmark) when it comes.
- e5 is weak one way across languages: a Ukrainian query put an English note on car insurance
  third of four in the embedder's test (EmbeddingGemma 2 first); the benchmark's English
  queries found their Ukrainian notes.
- The benchmark runs both models and holds floors for each: e5 by meaning 0.96 in both
  searches, at most 7.5 notes a query in the list; EmbeddingGemma 2 0.95, 6.8.
- 56 of the 65 queries by meaning were written by the developer from the vault's text: the
  comparison between models is fair (the same queries, no model tuned to them), the absolute
  numbers on real notes may be lower.

## Later (2026-11-17): EmbeddingGemma 2's floor on another Mac

- The benchmark, run for the first time on the owner's other Mac once `scripts/setup.sh` had
  fetched the models there (it skips without them), failed: EmbeddingGemma 2 found 61 of 65
  by meaning, both in the tools' search and in the list's, under its floor of 62. Bisected to
  the commit giving the tools' search the list's thresholds (2026-11-15), which measured 62
  here with the MRR it has there (0.81): the same code, one note standing out 1.68 to 1.7 on
  that Mac, at the threshold (1.7). Three runs gave the same; e5-large-instruct passes as it
  did (63 of 65).
- No threshold passes both floors there: 1.67 finds the note and gives 5.3 documents to a query
  with no answer (the floor 5.2), 1.68 gives 5.2 and loses it. The thresholds stay; the floor
  by meaning for EmbeddingGemma 2 is 61 of 65 for both searches (0.93: 0.938, rounded down as
  the other floors are), the numbers of both Macs beside it.

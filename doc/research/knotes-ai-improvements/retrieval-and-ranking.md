# Retrieval and ranking for personal-notes AI harnesses (state as of Nov 2026), and what fits Knotes

Scope: how leading notes/knowledge-base harnesses retrieve and rank, with numbers, for a local-first macOS app (Rust, SQLite FTS5 over ~2 KB blocks, up to 50k notes / ~200k blocks, ~300 MB RAM budget, M1 baseline, many Ukrainian notes). Knotes' own offline benchmark: Recall@5 0.97 on 37 lexical queries, 0/9 on "meaning" queries (synonyms, cross-language).

Dates: items marked (2024) or (2025) are older than a year; everything else is 2026 unless stated.

---

## 1. Hybrid search (BM25 + vectors) in notes harnesses: implementations, fusion, bonuses, rerankers, latency

### Takeaway
The most-copied local design in 2026 is qmd's: SQLite FTS5 (BM25) + sqlite-vec vectors (EmbeddingGemma-300M by default), LLM query expansion, RRF (k=60) with the original query weighted ×2 and a top-rank bonus, then a Qwen3-Reranker-0.6B blended by rank position. Lighter tools (Basic Memory) use a simple score fusion `max + 0.3·min` with an English-only small embedder and keep rerankers off by default; Obsidian Copilot V4 dropped its in-plugin index in favour of agent file tools plus a separate local semantic service (Miyo). Every serious notes tool keeps a semantic layer; nobody relies on lexical alone.

### Cited Findings
**qmd (Tobi Lütke) — fullest local hybrid pipeline**
- Lexical: SQLite FTS5 table `documents_fts`, BM25 scores range 0 to ~25+. Vectors: sqlite-vec index, one row per chunk. Single SQLite file `~/.cache/qmd/index.sqlite` (tables include `documents`, `documents_fts`, `content_vectors`, `vectors_by_collection`, `llm_cache`) — [qmd README](https://github.com/tobi/qmd)
- Embedder: `embeddinggemma-300M-Q8_0` (~300 MB) default; optional `Qwen3-Embedding-0.6B` via `QMD_EMBED_MODEL` (requires re-embedding). Query prompt `task: search result | query: {query}`, document prompt `title: {title} | text: {content}`. Cosine distance mapped to `1/(1+distance)` — [qmd README](https://github.com/tobi/qmd)
- Chunking: ~900 tokens, 15% overlap; break points scored (H1=100, H2=90, H3=80 … code fence=80, blank line=20, list item=5) searched in a 200-token window before the cutoff with squared-distance decay `final = base × (1 − (d/window)² × 0.7)`; code fences never split — [qmd README](https://github.com/tobi/qmd)
- Query expansion: fine-tuned `qmd-query-expansion-1.7B-q4_k_m` (~1.1 GB) emits typed sub-queries `lex` (→BM25), `vec` and `hyde` (→vectors); original query goes to both backends. (README is internally inconsistent about how many variants: "2 alternative queries" vs "+1 LLM variation".) — [qmd README](https://github.com/tobi/qmd)
- Fusion: RRF `score = Σ 1/(k+rank+1)`, **k=60**; **original query ×2 weight**; **top-rank bonus +0.05 for rank #1 in any list, +0.02 for ranks #2–3**; top 30 go to the reranker — [qmd README](https://github.com/tobi/qmd)
- Reranker: `qwen3-reranker-0.6b-q8_0` (~640 MB), candidate limit default 40. Position-aware blending (RRF weight / reranker weight): ranks 1–3 75%/25%, ranks 4–10 60%/40%, ranks 11+ 40%/60% — i.e. the reranker is trusted less at the top where lexical/fusion signal is already strong — [qmd README](https://github.com/tobi/qmd)
- Resource notes: three GGUF models ≈ 2.0 GB on disk (300 MB + 640 MB + 1.1 GB); models stay loaded; ~1 s penalty to recreate contexts after 5 min idle. No latency or RAM figures published. Example-fixture precision: ~0.50 BM25, ~0.70 vector, ~1.00 full hybrid (toy fixture, not a benchmark) — [qmd README](https://github.com/tobi/qmd)

**Basic Memory (MCP knowledge base over Markdown)**
- "Hybrid full-text + vector ranking with FastEmbed embeddings, on SQLite or Postgres" — [Basic Memory README](https://github.com/basicmachines-co/basic-memory)
- Default embedder FastEmbed `BAAI/bge-small-en-v1.5`, 384 dims, ONNX, local (English-oriented); OpenAI `text-embedding-3-small` 1536 dims optional; a multilingual MiniLM option has "materially higher memory use" — [Basic Memory semantic-search docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)
- Chunking: headers start chunks, each bullet is its own chunk, prose merged up to ~900 characters, oversized sections split with ~120-char overlap — [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)
- Storage: sqlite-vec `vec0` on SQLite; pgvector HNSW on Postgres — [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)
- Fusion is score-based, not RRF: FTS scores normalised to [0,1], `fused = max(vec_score, fts_score) + 0.3 * min(vec_score, fts_score)`; `semantic_vector_k` default 100; similarity cutoff 0.55; unrelated notes scored at most 0.67 to their nearest neighbour — [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)
- Reranker optional, **off by default**: FastEmbed cross-encoder `jinaai/jina-reranker-v1-tiny-en` over top 20 candidates (or Cohere rerank-v3.5 via LiteLLM); reranker scores replace retrieval scores for the reranked prefix; capping reranker input at 2000 chars measured identical to unbounded on LoCoMo, 1000 chars cost ~1.4 points recall@5; text/title/permalink searches keep their own ranking — [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md), [README](https://github.com/basicmachines-co/basic-memory)

**Obsidian Copilot (V4, 2026)**
- V4 "uses ordinary file tools for exact text and file lookup" with no index; the V3 in-plugin index is retired and its legacy index files removed on upgrade. Obsidian-aware tool (links, backlinks, properties, tags, tasks, Bases) uses Obsidian's own index — [Copilot docs: vault search and indexing](https://github.com/logancyang/obsidian-copilot/blob/master/docs/vault-search-and-indexing.md)
- Semantic search moved to **Miyo**, a local-first service; agents use it when "Semantic search for agents" is on; "Search variants" setting adds 0–2 rephrasings per search; max results 1–20; Relevant Notes keeps Miyo's order and similarity %, updates after a 500 ms typing pause; Miyo also parses PDF/EPUB. No embedding model, fusion or reranker details published — [Copilot docs](https://github.com/logancyang/obsidian-copilot/blob/master/docs/vault-search-and-indexing.md), [Copilot README](https://github.com/logancyang/obsidian-copilot)

**Smart Connections (v4)**
- Semantic-only: "A local embedding model powers semantic search"; explicitly warns "a note that contains the exact query text might not appear if it is not actually similar in meaning". Pro adds configurable scoring and an optional rerank stage; data in `.smart-env/` — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
- Default model unclear: fork Open Connections lists `Xenova/bge-micro-v2` as default ([Open Connections](https://community.obsidian.md/plugins/open-connections)); a third-party article says quantized all-MiniLM-L6-v2, 384 dims, block-level embeddings stored as JSON ([Starlog, third-party](https://starlog.pages.dev/articles/data-knowledge/brianpetro-obsidian-smart-connections)). Both are English-centric small models.

**Khoj** — bi-encoder for retrieval + "slower but higher-quality cross-encoder" reranker; docs say defaults are general-purpose and multilingual search needs a config change; no keyword/hybrid mentioned — [Khoj docs: search](https://docs.khoj.dev/features/search)

**Reor** — LanceDB + Transformers.js embeddings, semantic-only, related notes by vector similarity; **repository archived 7 Mar 2026** — [Reor GitHub](https://github.com/reorproject/reor)

**Fusion research**
- Bruch, Gai, Ingber (Pinecone; 2022/2023): convex combination (CC) of normalised lexical+semantic scores outperforms RRF in- and out-of-domain; RRF is sensitive to its parameters and a tuned RRF generalises poorly; CC needs only a small set of examples to tune its single weight and is largely agnostic to normalisation choice — [arXiv 2210.11934](https://arxiv.org/abs/2210.11934)

**Rerankers (numbers)**
- Qwen3-Reranker on top-100 from Qwen3-Embedding-0.6B: 0.6B: MTEB-R 65.80, MMTEB-R 66.36, MLDR 67.28; 4B: MMTEB-R 72.74; vs jina-multilingual-reranker-v2-base (0.3B) MMTEB-R 63.73, gte-multilingual-reranker-base 59.44, BGE-reranker-v2-m3 (0.6B) 58.36. Scoring = logits of "yes"/"no" after a chat-formatted prompt — [Qwen3-Reranker-0.6B card](https://huggingface.co/Qwen/Qwen3-Reranker-0.6B)
- Third-party small test (30 queries): EmbeddingGemma 2 top-15 + Qwen3-Reranker-4B raised nDCG@10 95.93→96.84 and Recall@5 92.22→95.56, rerank p50 ≈ 332 ms; with wrong prompt format (tags without ChatML) the reranker *dropped* nDCG@10 to 77.98 (−18.9 points) — [regolo.ai benchmark, vendor blog](https://regolo.ai/embeddinggemma-2-vs-qwen3-embedding-benchmarks.md)
- Practitioner (Ukrainian legal RAG): a strong embedder (Qwen3-Embedding-8B) let them switch the reranker off, cutting latency and dependencies — [DOU.ua article, Aug 2026](https://dou.ua/goto/Crco)

### Inferences
- For Knotes the qmd recipe is the right *shape* (FTS5 + vectors in SQLite + RRF + exact-match bonus), but its model footprint (~2 GB, three models) is ~7× Knotes' RAM budget; Knotes should take the fusion/bonus logic and skip the local 1.7B expander and probably the local reranker (the chat model already reads and filters results, acting as a reranker).
- qmd's "original query ×2 + top-rank bonus" and position-aware blending encode one lesson: protect strong exact matches from being displaced by semantic noise. This matters for Knotes, which already scores 0.97 on lexical queries — any hybrid must not regress them. Fusion should be evaluated on both the 37 lexical and 9 meaning queries.
- Knotes has a labelled benchmark, so CC with a tuned weight (Bruch) is feasible and cheap to tune; RRF k=60 is the safe untuned default.
- Basic Memory's `max + 0.3·min` is a cheap, deterministic alternative that also rewards agreement between both retrievers.

### Gaps
- No published latency for qmd, Miyo, Smart Connections or Basic Memory hybrid queries.
- Miyo's embedding model, fusion and reranker are undocumented; Smart Connections' current default model is unconfirmed (sources conflict).
- I did not research Seek / "Vault Search" Obsidian plugins (time budget); no data on them here.

---

## 2. Anthropic Contextual Retrieval and cheap variants (title / heading path)

### Takeaway
Anthropic's numbers (2024) remain the reference: contextual embeddings cut top-20 retrieval failures 35%, adding contextual BM25 49%, adding a reranker 67%. Per-chunk LLM context is unaffordable/undesirable for a private 200k-block vault via a ChatGPT subscription, but a 2026 placebo-controlled study shows real heading paths + structure-aligned chunks give a small, significant gain for free; EmbeddingGemma's native `title:` field is the natural place for note title + heading path.

### Cited Findings
- Baseline top-20 failure rate 5.7%. Contextual Embeddings → 3.7% (−35%); Contextual Embeddings + Contextual BM25 → 2.9% (−49%); + reranking → 1.9% (−67%) — [Anthropic: Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval) (published 2024)
- Cost $1.02 per million document tokens one-time (with prompt caching; assumes 800-token chunks, 8k-token docs, 50–100 tokens of context per chunk) — [Anthropic](https://www.anthropic.com/engineering/contextual-retrieval)
- Retrieval setup: top-150 retrieved, reranked to top-20; 20 chunks to the model beat 5 and 10; Gemini and Voyage embeddings best; Cohere reranker — [Anthropic](https://www.anthropic.com/engineering/contextual-retrieval)
- Knowledge bases under 200k tokens (~500 pages) can go wholly in the prompt without RAG; prompt caching cuts latency >2× and cost up to 90% — [Anthropic](https://www.anthropic.com/engineering/contextual-retrieval)
- Placebo-controlled ablation (Kuehlkamp et al., 7 Oct 2026, preprint "initial draft"): structure-aligned chunks with real heading paths vs contextualised fixed windows +0.022 / +0.012 cov-nDCG@10 (Wikipedia / QASPER); real vs shuffled-heading placebo +0.010 / +0.016; naive two-stage hierarchical retrieval −0.033 / −0.015; effect sizes small (dz 0.06–0.11) but Holm-significant; gold structure beats LLM-induced structure on Wikipedia, not on QASPER; cutting at section boundaries matters more than adding text — [arXiv 2610.10170](https://arxiv.org/abs/2610.10170)
- Metadata benchmark: section titles give "more modest gains" than strongly disambiguating fields (company, year) — [RAGMATE-10K, arXiv 2601.11863](https://arxiv.org/html/2601.11863v1)
- Practitioner note: LangChain's MarkdownHeaderTextSplitter strips headings from chunk text by default, hurting BM25 in hybrid setups; prepending title + heading path is free (no benchmark) — [dev.to, practitioner](https://dev.to/kartikeyraj/free-contextual-chunk-headers-heading-aware-chunking-for-hybrid-retrieval-560)
- EmbeddingGemma's document prompt is `title: {title | "none"} | text: {content}` — the model is trained to use a title field — [EmbeddingGemma card](https://huggingface.co/google/embeddinggemma-300m); qmd fills it with the document title — [qmd README](https://github.com/tobi/qmd)
- Without instruction/prompt prefixes, Recall@5 fell 5.0 points (EmbeddingGemma 2), 2.78 (Qwen3-0.6B), 4.44 (Qwen3-8B) in a small third-party test — [regolo.ai](https://regolo.ai/embeddinggemma-2-vs-qwen3-embedding-benchmarks.md)

### Inferences
- Cheap Knotes variant: for each ~2 KB block, embed `title: <note title> › <H1> › <H2> | text: <block>`; for FTS5, index the heading path (and aliases) as a separate weighted column so BM25 on a block sees the note's topic. This approximates "contextual BM25" at zero LLM cost.
- Knotes already chunks by blocks; aligning block boundaries with headings (not fixed 2 KB windows) is the part with measured benefit.
- LLM-written per-block context (Anthropic style) at 200k blocks ≈ 40–100M tokens through the user's ChatGPT quota (my estimate) — impractical and slow; skip, or limit to PDFs where structure is missing.
- The "<200k tokens → put it all in the prompt" rule could apply to small folder-scoped questions, not to whole vaults.

### Gaps
- No benchmark directly comparing "title+heading prefix" vs "LLM context" on personal notes or Ukrainian text.

---

## 3. Query expansion and HyDE for personal-notes search

### Takeaway
Expansion helps weak retrievers (BM25 facing synonyms/cross-language) and tends to hurt strong ones by diluting the query. In agent harnesses, the agent's own query rewriting is the cheapest expansion; Copilot V4 caps it at 0–2 variants and qmd routes typed variants (lex/vec/hyde) to different backends while double-weighting the original.

### Cited Findings
- Weller et al. (EACL Findings 2024): across 11 expansion techniques, 12 datasets, 24 retrievers there is a strong negative correlation between retriever strength and gain from LLM expansion; expansions add noise that makes it harder to separate top relevant documents; recommended for weaker models or out-of-format data, otherwise avoid; exception: long queries — [arXiv 2309.08541](https://arxiv.org/abs/2309.08541)
- qmd: fine-tuned 1.7B expander emits `lex`, `vec`, `hyde` sub-queries; original query sent to both backends and weighted ×2 in RRF — [qmd README](https://github.com/tobi/qmd)
- Obsidian Copilot V4: "Search variants" = how many rephrasings a search can add, 0 to 2 — [Copilot docs](https://github.com/logancyang/obsidian-copilot/blob/master/docs/vault-search-and-indexing.md)
- Agentic search (agent issues many grep/search calls) trades latency and tokens for quality — [Latent Space, Boris Cherny, May 2025](https://www.latent.space/p/claude-code)

### Inferences
- Knotes' 0/9 meaning queries are exactly the "weak retriever" case: until vectors exist, telling the chat model to search each concept in Ukrainian *and* English with 1–2 synonyms (e.g., "котел OR водонагрівач OR бойлер", "car insurance OR страховка авто") is near-zero effort and costs only extra tool calls; it should recover part of the 9 queries.
- Once a dense retriever is added, automatic expansion should be restrained (original query dominant), per Weller; HyDE mostly duplicates what a multilingual embedder already does for synonyms.
- A local expander model (qmd's 1.1 GB) does not fit the 300 MB budget and is redundant when an LLM agent already writes the queries.

### Gaps
- No measurements found of HyDE or expansion specifically on personal-notes corpora or Ukrainian; no latency numbers for qmd's expansion step.

---

## 4. Multilingual embeddings for Ukrainian on Apple Silicon: quality, dims, quantization, memory, throughput

### Takeaway
Best fit for Knotes: EmbeddingGemma (v1 300M, or v2's 270M text component, Apache 2.0, Oct 2026) — strongest open multilingual model under ~500M, <200 MB RAM quantized, 768→256/128 Matryoshka with ~1–2 point loss. Qwen3-Embedding-0.6B scores higher (MMTEB 64.33 vs 61.2) but is 2× larger; Qwen3-4B/8B lead Ukrainian retrieval but are too big. Jina v5 is strong but CC-BY-NC. Ukrainian-specific evidence is thin: MTEB's Ukrainian retrieval filter and one legal-domain benchmark both rank Qwen3 > multilingual-e5-large ≈ BGE-M3 >> OpenAI te3-small.

### Cited Findings
**Model facts and multilingual scores (MTEB Multilingual / MMTEB mean task)**
- EmbeddingGemma-300M (4 Sep 2025): 308M params (~100M model + ~200M embedding params), 100+ languages, 2K context, <200 MB RAM with quantization, <15 ms per 256-token embedding on EdgeTPU; "highest ranking open multilingual text embedding model under 500M"; runs on MLX, llama.cpp, Ollama, transformers.js, LiteRT — [Google Developers Blog](https://developers.googleblog.com/en/introducing-embeddinggemma/)
- EmbeddingGemma-300M MTEB Multilingual v2: 768d 61.15, 512d 60.71, 256d 59.68, 128d 58.23; QAT Q8_0 60.93, Q4_0 60.62, mixed 60.69 (768d) — [HF card](https://huggingface.co/google/embeddinggemma-300m)
- EmbeddingGemma 2 (released 6 Oct 2026, Apache 2.0, ungated; Gemma 4-based): 740M total, text-only component 270M (130M backbone + 140M embedder), 8,192-token context, 100+ languages (training data "over 140 languages"); MTEB Multilingual v2 768d 61.36, 512d 61.17, 256d 60.41, 128d 57.89; Code v1 78.68 (v1: 68.76); avoid float16 (NaNs) — use bf16 — [HF card embeddinggemma-2](https://huggingface.co/google/embeddinggemma-2); release date/licence change via [mixed-news](https://mixed-news.com/en/?p=35813), [vpsranking](https://vpsranking.com/news/ai/ai-2026-10-06-google-embeddinggemma-2/) (secondary). Japanese press reports minimum RAM ~191 MB ([livedoor, secondary](https://news.livedoor.com/article/detail/32509507/)).
- Qwen3-Embedding-0.6B: 28 layers, 32K context, dims up to 1024 with user-defined 32–1024 (MRL), instruction-aware; MMTEB mean: 8B 70.58, 4B 69.45, 0.6B 64.33, multilingual-e5-large-instruct 63.22, BGE-M3 59.56 (competitor scores from leaderboard May 2025) — [Qwen3-Embedding-0.6B card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B)
- BGE-M3: 568M params, 1024 dims, 8192-token input; multilingual-E5 models capped at 512 tokens — [Russian IR benchmark paper, arXiv 2504.12879](https://arxiv.org/pdf/2504.12879)
- jina-embeddings-v5-text (18–19 Feb 2026): small 677M, MMTEB 67.0, 32K ctx, MRL down to 32; nano 239M, MMTEB 65.5, 8K ctx; ONNX/GGUF/MLX builds; **licence CC-BY-NC-4.0** (confirmed for small; nano unconfirmed) — [Jina release note](https://jina.ai/news/jina-embeddings-v5-text-distilling-4b-quality-into-sub-1b-multilingual-embeddings), [Jina model page](https://jina.ai/en-US/models/jina-embeddings-v5-text-small/), [HF nano card](https://huggingface.co/jinaai/jina-embeddings-v5-text-nano)

**Ukrainian-specific evidence**
- DOU.ua practitioner (26 Aug 2026), MTEB filtered to Ukrainian retrieval: Qwen3-Embedding-8B 98.45, Gemini Embedding 001 96.26, Qwen3-Embedding-4B 94.46, multilingual-e5-large 93.73, text-embedding-3-large 92.45, bge-m3 92.40, text-embedding-3-small 80.60. Ukrainian MTEB tasks cover general domains (Wikipedia, news, reviews) — [DOU.ua](https://dou.ua/goto/Crco)
- Same author's legal-domain benchmark (203 queries, 11,516 chunks), Recall@5: te3-small 78.3%, bge-m3 81.3%, Qwen3-4B 89.7%, Qwen3-8B@2048 93.1% (@3072: 92.1% — non-standard MRL dims can be worse). MTEB overestimated bge-m3 (+12 promised, +3 real). Building a gold set with an LLM cost < $0.50. Qwen3-0.6B and EmbeddingGemma not tested — [DOU.ua](https://dou.ua/goto/Crco)
- No peer-reviewed Ukrainian retrieval benchmark comparing small models found; SED-UA-Small is a synthetic Ukrainian STS/embedding dataset ([Lviv Polytechnic](https://science.lpnu.ua/node/39142)); CrossNews-UA covers cross-lingual news similarity uk/pl/ru/en ([arXiv 2510.19628](https://arxiv.org/pdf/2510.19628))

**Small third-party head-to-head (30 queries, 60 docs, IT/EN + code — noisy)**
- nDCG@10 / Recall@5: EmbeddingGemma 2 95.93 / 92.22; EmbeddingGemma 1 97.17 / 92.22; Qwen3-0.6B 97.76 / 95.00. MRL for Qwen3-0.6B flat down to 256d (97.54 nDCG); EmbeddingGemma 2 Recall@5 drops 92.22→87.22 at 256d — [regolo.ai](https://regolo.ai/embeddinggemma-2-vs-qwen3-embedding-benchmarks.md)
- Query latency p50: EmbeddingGemma 2 bf16 on MLX (M1 Ultra) 23.9 ms, 542 MB; EmbeddingGemma 1 llama.cpp 23.7 ms, 685 MB; Qwen3-0.6B llama.cpp 30.1 ms, 5.8 GB (inflated by pre-allocating 32K context) — [regolo.ai](https://regolo.ai/embeddinggemma-2-vs-qwen3-embedding-benchmarks.md)

**Quantization of stored vectors (2024 study)**
- int8 scalar: ~97% of float32 retrieval quality without rescoring, ~99% with rescoring (×4 oversample), 4× smaller, ~3.7× faster; binary: ~92.5% → ~96% with rescoring for mxbai-embed-large, 32× smaller, ~25× faster — but model-dependent (e5-base-v2 binary only 74.77%) — [HF blog: Embedding Quantization](https://huggingface.co/blog/embedding-quantization)

**Throughput on Apple Silicon (weak evidence)**
- Qwen3-Embedding-0.6B 4-bit on M4 (10-core GPU): ~1,864 tok/s prefill on a 4k prompt (community run) — [oMLX community benchmarks](https://omlx.ai/benchmarks/eb3gut7d); unverified claim of 44,000+ tok/s with no methodology — [qwen3-embeddings-mlx](https://github.com/jakedahn/qwen3-embeddings-mlx); Core ML fp16 export ~25 ms (256 tokens) / ~45 ms (512) per embedding on M4 Max GPU — [HF Core AI export](https://huggingface.co/mlboydaisuke/Qwen3-Embedding-0.6B-CoreAI)
- One MLX 6-bit conversion of Qwen3-0.6B failed its cosine-fidelity gate — quantization can damage embeddings — [search result summary of TiGa-RCE card](https://huggingface.co/TiGa-RCE/Qwen3-Embedding-0.6B-MLX-oQ6)

### Inferences
- Memory for 200k blocks (my arithmetic): 768d f32 614 MB; f16 307 MB; int8 154 MB; **256d int8 51 MB**; 768d binary 19 MB; Qwen3 1024d f32 819 MB. Only int8 at ≤512d (or binary + int8 rescoring) fits beside the model within ~300 MB.
- Model RAM dominates: EmbeddingGemma quantized <200 MB is the only candidate that leaves room; Qwen3-0.6B Q8 (~600 MB) does not fit always-resident. Load the embedder on demand (index/query) and unload after idle, as qmd does with its 5-min context recycle.
- Indexing cost (rough estimate): ~2 KB of Cyrillic ≈ 1,000 chars ≈ 300–600 tokens per block → ~60–120M tokens for 200k blocks; at ~2k tok/s that is ~8–17 h, at ~10k tok/s ~2–3 h. Initial embedding must be a resumable background job; incremental updates thereafter are tiny. Needs a measurement on M1.
- Cross-language ("car insurance" ↔ "страховка") is precisely what multilingual embedders are trained for; Knotes' 9 meaning queries become the acceptance test. Expect EmbeddingGemma vs Qwen3-0.6B to differ by a few points; Knotes should benchmark both on its own queries, as the DOU author recommends.
- Avoid: Jina v3/v4/v5 (non-commercial licence), BGE-M3 (568M, weaker than Qwen3-0.6B, overestimated on Ukrainian), multilingual-e5 (512-token cap is fine for 2 KB blocks, but older and ~560M for large), English-only defaults like bge-small-en (Basic Memory) or bge-micro (Smart Connections).

### Gaps
- No M1-base (8-core GPU) embedding throughput figures for EmbeddingGemma or Qwen3-0.6B in MLX, Core ML, ONNX or candle; must be measured.
- No Ukrainian retrieval scores published for EmbeddingGemma (v1/v2) or Qwen3-0.6B specifically; Nomic and multilingual-e5-small not researched.
- Qwen3-Embedding licence not verified in this session (believed Apache 2.0).

---

## 5. Vector storage in SQLite/Rust and keeping indexes fresh

### Takeaway
At 200k vectors, exact brute-force search is enough: sqlite-vec (stable = brute force; ANN only in 2026 alphas) queries 100k×768 float vectors in <75 ms from disk (2024, M1), binary vectors in ~11 ms. An in-memory SIMD scan over int8/f16 in Rust is simpler and faster still; HNSW (usearch) adds memory and update complexity that 200k vectors do not need. Freshness via per-chunk content hashes (Basic Memory) is the standard pattern.

### Cited Findings
- sqlite-vec stable release (2024): brute-force only; float and bit vectors (`vec_quantize_binary`, 32× smaller, author estimates ~5–10% quality loss for ~10× faster queries). 100k vectors disk-backed: 3072d 214 ms, 1536d 105 ms, ≤1024d <75 ms; bit 3072d 11 ms; 1M float vectors: 192d 192 ms, 3072d 8.52 s. sift1m (1M×128, M1 Mac mini, in-memory): faiss 10 ms, sqlite-vec static 17 ms, vec0 33 ms, usearch exact 56 ms — [sqlite-vec stable release post](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html)
- sqlite-vec 2026: v0.1.7 (17 Mar) proper DELETE that reclaims space per ~1024-vector chunk; v0.1.9 (31 Mar) metadata-text-column fixes; v0.1.10-alpha.1 (31 Mar) adds ANN indexes "rescore, ivf (experimental, not enabled), and DiskANN"; alpha.4 (18 May) still alpha. Rust crate available — [sqlite-vec releases](https://github.com/asg017/sqlite-vec/releases), [sqlite-vec docs](https://alexgarcia.xyz/sqlite-vec/)
- qmd and Basic Memory both store vectors in sqlite-vec next to FTS5 in one SQLite file — [qmd](https://github.com/tobi/qmd), [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)
- usearch: proximity-graph index, Rust bindings, quantization types bf16, u1, i8, e4m3 (plus f16, u8, i4 …), exact scans supported, insert grows the index; SIFT-100M on 192 cores builds 161,952 vec/s and searches 173,823 QPS vs FAISS HNSW 84,536 / 178,600 — [usearch GitHub](https://github.com/unum-cloud/usearch)
- Freshness: Basic Memory stores a SHA-256 `source_hash` per chunk and skips re-embedding unchanged chunks on re-sync; notes >256 chunks embedded in shards of 256; changing model/dims requires a full re-embed — [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md). Changing qmd's model also requires `qmd embed -f` — [qmd](https://github.com/tobi/qmd)
- Staleness of embeddings for edited files was one reason Claude Code dropped its vector index ("the code drifts out of sync") — [Latent Space, May 2025](https://www.latent.space/p/claude-code)

### Inferences
- Knotes option A (simplest, fastest): keep vectors as int8 (or f16) BLOBs in a SQLite table keyed by block id + content hash; load into one contiguous array at startup (51 MB at 256d int8); scan with NEON dot products. 200k×256 int8 ≈ 51M multiply-adds per query — low single-digit ms on M1 (estimate). Optional binary pre-filter + int8 rescoring if dims grow.
- Option B: sqlite-vec `vec0` (one dependency, SQL-level filtering) — fine but adds a C extension; its ANN features are alpha.
- HNSW/usearch only pays off at millions of vectors; at 200k it costs extra RAM for the graph and complicates deletes.
- Knotes already re-indexes blocks for FTS5 on file change; reuse that path: hash each block, re-embed only changed blocks, queue in background, and let FTS5 cover the gap until embeddings catch up (lexical is always fresh).

### Gaps
- No 2026 benchmarks of sqlite-vec's DiskANN/rescore on Apple Silicon; no published numbers for a Rust NEON int8 scan at 200k (needs a microbenchmark).

---

## 6. "Agentic grep vs embeddings": what applies to prose notes

### Takeaway
Code agents' preference for grep (Claude Code) rests on exact identifiers, freshness and security; even so, Cursor measured +12.5% average accuracy from adding semantic search to grep (Nov 2025). Notes are prose with heavy vocabulary mismatch and, for Knotes, cross-language content, so embeddings add more there than in code; Obsidian Copilot V4 made exactly that split — index-free file tools for exact lookup, kept semantic search for meaning.

### Cited Findings
- Claude Code: early versions used RAG with an off-the-shelf index ("I think we were just using Voyage"); dropped because the index drifts out of sync and for security concerns about where it lives; "we landed on just agentic search" (glob, grep, repeated); "outperformed everything" per "internal vibes" plus some internal benchmarks; costs latency and tokens — [Latent Space with Boris Cherny, 7 May 2025](https://www.latent.space/p/claude-code). Frequently quoted tweet version ("agentic search generally works better… simpler… security, privacy, staleness, reliability") relayed by secondary sources — [besthub.dev, secondary](https://www.besthub.dev/articles/is-rag-dead-how-grep-is-making-a-comeback-in-llm-powered-code-search-695a8ea40e1b)
- Cursor (6 Nov 2025): semantic search alongside grep gives on average 12.5% higher accuracy (6.5%–23.5% by model) on Cursor Context Bench; online A/B: +0.3% code retention overall, +2.6% on codebases ≥1,000 files; 2.2% more dissatisfied follow-ups without semantic search; "the combination of these two leads to the best outcomes"; most useful for natural-language queries — [Cursor blog](https://cursor.com/blog/semsearch)
- Zilliz/Milvus claim ~40% token reduction with its claude-context MCP (hybrid BM25+vector) vs grep-only — vendor claim, baseline varies by source, not independently replicated — [Zilliz blog](https://zilliz.com/blog/why-im-against-claude-codes-grep-only-retrieval-it-just-burns-too-many-tokens), [third-party review](https://toolbrain-astro.pages.dev/blog/zilliztech-claude-context-review-2026-semantic-code-search-mcp-for-claude-code/)
- Obsidian Copilot V4: file tools for exact text, Obsidian CLI for links/tags, Miyo for semantic search — [Copilot docs](https://github.com/logancyang/obsidian-copilot/blob/master/docs/vault-search-and-indexing.md)
- Smart Connections (semantic-only) warns exact-text notes may not appear — the inverse failure — [Smart Connections README](https://github.com/brianpetro/obsidian-smart-connections)
- 2026 benchmark: agentic search substantially improves dense RAG and narrows the gap to GraphRAG — [arXiv 2604.09666 (alphaxiv)](https://www.alphaxiv.org/abs/2604.09666.md)

### Inferences
- Knotes' own data settles this for its corpus: lexical/agentic search is excellent on lexical queries (0.97) and useless on meaning queries (0/9). Agentic rewriting can patch some of the 9; embeddings address the root cause.
- The Claude Code objections are weaker for Knotes: the index is local (no privacy issue), Knotes already keeps an FTS5 index fresh, and incremental re-embedding of changed blocks is cheap after the initial build.
- Keep Knotes' search as an agent tool (agent can call it repeatedly), but make it hybrid underneath, or expose a separate `semantic_search` tool so the model picks lexical vs semantic like Cursor's agent.

### Gaps
- No published evaluation of agentic grep vs embeddings on personal-notes corpora; evidence is from code (Cursor, Claude Code) or vendor claims.

---

## 7. GraphRAG / LightRAG / knowledge graphs for a personal vault

### Takeaway
GraphRAG wins mainly on multi-hop/global questions, loses on single-hop factual lookup (the typical notes query), and needs LLM entity extraction over the whole corpus — expensive, slow to update (GraphRAG) and privacy-sensitive. With a larger retrieval budget plain RAG nearly matches it. A personal Obsidian vault already has a free, human-made graph (wiki-links, backlinks, tags), which Knotes already uses for link expansion.

### Cited Findings
- Han et al. "RAG vs. GraphRAG: A Systematic Evaluation" (v3, Mar 2026): RAG better on single-hop and detail-oriented factual queries; GraphRAG better on multi-hop reasoning; "complementary behaviors rather than a consistent winner"; token-matched RAG 71.01% vs Community-GraphRAG (Local) 71.17% overall (Llama 3.1-70B), no significance test; GraphRAG has higher construction cost, retrieval latency and storage, and is sensitive to graph quality — [arXiv 2502.11371](https://arxiv.org/abs/2502.11371)
- Graph retrieval reported to add ~2.3× latency on average — [arXiv 2506.05690](https://arxiv.org/pdf/2506.05690v3)
- 2026: agentic search narrows dense-RAG gap to GraphRAG; GraphRAG keeps an advantage on complex multi-hop when offline cost is amortised — [arXiv 2604.09666](https://www.alphaxiv.org/abs/2604.09666.md)
- LightRAG supports incremental addition without rebuilding the graph (GraphRAG requires rebuilding); a comparison table cites LightRAG indexing at 35.52M prompt + 51.16M completion tokens on its test corpus — [WikiDocs course note, secondary](https://wikidocs.net/352215). LazyGraphRAG "0.1% of GraphRAG indexing cost" appears only in secondary sources — unverified.
- Copilot V4 exposes Obsidian's own link/backlink/tag index as an agent tool rather than building an LLM graph — [Copilot docs](https://github.com/logancyang/obsidian-copilot/blob/master/docs/vault-search-and-indexing.md)

### Inferences
- For Knotes: do not build GraphRAG/LightRAG. Indexing 50k notes with LLM entity extraction through the user's ChatGPT quota is tens of millions of tokens (cf. LightRAG's ~87M tokens on a test corpus), re-runs on edits, and sends the whole vault to the cloud.
- Cheap graph wins already available: backlinks/outlinks/tags as tools (Knotes has "what is connected with a note"), link expansion as a fallback tier, and possibly a small graph-proximity boost in ranking (e.g., blocks from notes linked to top hits).

### Gaps
- No evaluation of GraphRAG on personal knowledge bases found; cost figures are from secondary sources.

---

## 8. Ukrainian/Russian morphology in lexical search

### Takeaway
Snowball added an official Ukrainian stemmer in v3.2.0 (17 Oct 2026); Russian has long been in Snowball. This is a direct replacement for Knotes' "cut 1–2 letters" heuristic and is available in Rust (snowball_stemmers_rs lists Ukrainian). Dictionary lemmatisation (dict_uk/VESUM, used by Lucene's UkrainianMorfologikAnalyzer and pymorphy3) is more accurate but needs a large dictionary.

### Cited Findings
- Snowball algorithms list includes Ukrainian and Russian (Slavic group with Czech, Polish, Serbian) — [snowballstem.org/algorithms](https://snowballstem.org/algorithms/)
- "Add Ukrainian stemmer from Dmitry Prudnikov (#277)" — Snowball **3.2.0, 2026-10-17** — [Snowball NEWS](https://raw.githubusercontent.com/snowballstem/snowball/master/NEWS)
- Ukrainian stemmer design: modelled on the Russian stemmer; regions pV and R2; removes perfective gerunds (-вши/-вшись), reflexive endings, adjective/participle endings (derivational -аційн-, -ічн- only in R2), verb endings with thematic vowel ("читала", "читає" → "чит"), noun endings, -ість abstract nouns; tuned on a Ukrainian Wikipedia frequency list; limitations: fleeting vowels and suppletive plurals handled only as whole-word exceptions, apostrophes kept (normalise apostrophe variants first), rare words less reliable; no accuracy figures — [Snowball Ukrainian stemmer](https://snowballstem.org/algorithms/ukrainian/stemmer.html)
- Rust: `snowball_stemmers_rs` lists Ukrainian among supported algorithms — [docs.rs](https://docs.rs/crate/snowball_stemmers_rs/latest)
- Before 2026 Ukrainian Snowball was absent (2022 Stack Overflow; Elasticsearch has a dictionary `ukrainian` analyzer plugin, not a Snowball stemmer) — [Elastic forum](https://discuss.elastic.co/t/ukrainian-analyzer/333062.md); third-party Ukrainian stemmers exist — [amakukha/stemmers_ukrainian](https://github.com/amakukha/stemmers_ukrainian), [Amice13/ukr_stemmer](https://github.com/Amice13/ukr_stemmer)
- Dictionary lemmatisation: Lucene's `UkrainianMorfologikAnalyzer` (since Lucene 6.2.0) uses a binary dictionary based on brown-uk/dict_uk — [Lucene javadoc](https://lucene.apache.org/core/8_5_1/analyzers-morfologik/org/apache/lucene/analysis/uk/UkrainianMorfologikAnalyzer.html), [Lucene.Net README](https://apache.googlesource.com/lucenenet/+/refs/tags/Lucene.Net_4_8_0_beta00012/src/Lucene.Net.Analysis.Morfologik/Uk/README); pymorphy3 has Ukrainian dictionaries (`lang='uk'`) — [pymorphy3-dicts-uk](https://pypi.org/project/pymorphy3-dicts-uk)
- qmd and Basic Memory use plain FTS5 without language-specific stemming (no stemmer mentioned) — [qmd](https://github.com/tobi/qmd), [Basic Memory docs](https://github.com/basicmachines-co/basic-memory/blob/main/docs/semantic-search.md)

### Inferences
- Implementation for Knotes: a custom FTS5 tokenizer (or a stemmed shadow column) that detects script per token and applies Snowball Ukrainian or Russian to Cyrillic, English Porter2 to Latin; keep the unstemmed column too so exact/prefix matches still rank first (exact-match bonus). Uk vs ru per token is ambiguous; a cheap heuristic is letters і/ї/є/ґ → Ukrainian, ы/э/ё/ъ → Russian, otherwise Ukrainian (vault-dominant language).
- Effort low (compile-time Snowball code, no runtime memory), impact moderate: fixes inflection misses that the crude cutter misses or over-matches; does not fix synonyms or cross-language (that needs embeddings or agent expansion).
- Dictionary lemmatisation (dict_uk) is better for fleeting vowels/suppletion but adds a dictionary of unknown size and maintenance; defer unless the benchmark shows stemmer gaps.

### Gaps
- No measured retrieval gain of Snowball Ukrainian vs the crude cutter or vs dictionary lemmatisation; Knotes should measure on its 37+9 queries.
- dict_uk binary dictionary size and Rust availability not checked; whether the popular `rust-stemmers` crate already includes Ukrainian not verified.

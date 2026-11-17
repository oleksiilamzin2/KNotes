# Knotes

[Українською](README.uk.md)

Knotes is a native macOS app for notes kept as plain Markdown files in a folder. Open the
folder you already have (an Obsidian vault works as it is), and Knotes gives you a fast
editor, search that understands what you mean, and, if you want it, a chat that answers
questions about your notes.

Your notes stay ordinary files. Knotes never locks them into a database: the index it builds
lives outside the folder and can be thrown away at any time.

> Knotes is a young personal project (version 0.1). Its interface is in Ukrainian for now.

## What it does

**Writing**

- Live preview, as in Obsidian: the markup hides everywhere except on the line you are on.
- Formulas (LaTeX), images, PDF pages, tables you edit as a grid, highlighted code, tasks,
  callouts, footnotes.
- A note embedded with `![[note]]` shows in place, and you can edit it right there.
- It saves on its own two seconds after you stop typing. If another app changes the note
  meanwhile, both versions are kept. If a save fails, Knotes says so and will not let you
  close the note without asking.

**Finding**

- By file name, as you type.
- By the words in the text, with Ukrainian and English word forms: `мовою` finds `мова`.
- By meaning, with a model that runs on your Mac: "what to feed the cat" finds the note
  about the cat's food, even if it shares no words with the question.
- Backlinks, mentions that are not links yet (one click makes them links), and the outline
  of the note.

**Organising**

- Folders and tags, and tabs that are still there next time.
- Spaces: named, coloured sets of folders, such as "Work" and "Home".
- Renaming or moving a note updates every link to it.
- Deleted notes wait in "Recently deleted" (30 days unless you choose otherwise).

**AI, if you want it**

- A chat with an OpenAI model on your own ChatGPT plan, with no API key. It reads your notes
  through search and links to the ones it used; it cannot change them.
- Anything you choose is never shown to it: «Сховати від ШІ» in a note's, file's or folder's
  menu keeps it, and a small grey struck-through brain marks it. (Underneath, a note gets the
  `#no-ai` tag; a folder or a PDF is named in its folder's `.knotes.json`.)
- An MCP server, `knotes-mcp`, gives the same read-only tools to other agents, such as
  Claude Code.

## What you need

- A Mac with Apple silicon and macOS 27 or later.
- To build it: the Xcode Command Line Tools, Rust 1.99 or later, and cmake (the setup script
  installs cmake through Homebrew).
- For the chat: a ChatGPT plan. Everything else works without an account and offline.

## Build and run

```sh
git clone https://github.com/oleksiilamzin2/KNotes.git
cd KNotes
scripts/setup.sh     # cmake, and the two search-by-meaning models (about 900 MB)
scripts/bundle.sh    # builds target/release/Knotes.app
open target/release/Knotes.app
```

On first launch Knotes asks for a folder. Choose your notes (or an Obsidian vault). Knotes
indexes it in the background, so you can start at once.

Just want to try it without the models? Search by meaning is then off, everything else
works:

```sh
cargo run --release -p knotes -- ~/path/to/your/notes
```

`bundle.sh` signs the app with your Apple Development certificate if you have one (a free
personal team in Xcode is enough). Without one the app still runs, but after every build
macOS asks again before the chat may read its sign-in from the Keychain.

## Where your data lives

- **Your notes** stay where they were. Knotes adds only `.knotes.json` files (a folder's
  settings) and a hidden `.knotes` folder with pasted images and recently deleted notes.
  Both travel with the folder through iCloud or git.
- **The index, search vectors, chat conversations and tabs** are in
  `~/Library/Application Support/Knotes`, outside your folder. Delete them and Knotes builds
  them again.
- **What leaves your Mac:** only when you use the chat. Your questions and the passages its
  searches find go to OpenAI. Search by meaning runs on your Mac.

## Your notes in other agents

`knotes-mcp` sits next to the app, in `Knotes.app/Contents/MacOS`. Open your folder in Knotes
once (that builds the index), then add the server. For Claude Code:

```sh
claude mcp add knotes -- /Applications/Knotes.app/Contents/MacOS/knotes-mcp ~/path/to/your/notes
```

Use the path where your `Knotes.app` is. Other clients take the same command and argument in
their MCP settings. The tools only read: search, read a note or a PDF, links, notes by
folder, tag or date, tasks.

## For developers

The code is one Cargo workspace: libraries in `lib/`, programs in `bin/`.

| Where | What |
|---|---|
| `bin/knotes` | The app, AppKit through `objc2` |
| `bin/knotes-mcp` | The MCP server |
| `bin/knotes-embed-helper` | The helper process that runs the embedding model (llama.cpp on Metal) |
| `bin/knotes-gen`, `bin/knotes-bench` | Synthetic vaults and benchmarks |
| `lib/knotes-core` | The domain model and the traits everything else implements |
| `lib/knotes-md`, `lib/knotes-index`, `lib/knotes-vault`, … | Markdown, the SQLite index, the files, search, the editor model, the chat |

Why things are the way they are is written down in [`doc/adr`](doc/adr): one decision per
file, with what was measured. Start with
[0001](doc/adr/0001-workspace-layout-and-traits.md) for the layout and
[0006](doc/adr/0006-performance-budgets.md) for the performance budgets.

Tests:

```sh
cargo test --workspace                   # everything, the window tests included
cargo test --workspace --exclude knotes  # without the window tests: much faster
scripts/test.sh deck card                # the fast ones, then the window tests named so
scripts/test.sh all                      # everything
```

The window tests launch the real app hundreds of times; its windows are transparent then and it
shows no Dock icon, so they run in the background without getting in your way. Tests that need
the search-by-meaning models are skipped, and say so, until `scripts/setup.sh` has fetched them.

Benchmarks on a synthetic vault:

```sh
cargo run --release -p knotes-gen -- target/vaults/50k --notes 50000 --seed 1
cargo run --release -p knotes-bench -- target/vaults/50k
```

With 50,000 notes on an M5 Pro, a search by file name takes under 2 ms, a full-text search
about 10 ms, and indexing a vault from scratch under 7 seconds ([`doc/bench`](doc/bench)).

The licences of the bundled models and of llama.cpp are in [`licenses`](licenses).

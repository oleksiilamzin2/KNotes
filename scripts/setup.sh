#!/bin/sh
# Sets up a development machine (ADR 0028): the build tools Rust does not bring (cmake, for
# llama.cpp) and the files the build takes that git does not keep (the embedding models).
# Safe to run again: it only fetches what is missing or wrong.
# Usage: scripts/setup.sh
set -eu
cd "$(dirname "$0")/.."

if ! command -v cmake >/dev/null 2>&1; then
  if ! command -v brew >/dev/null 2>&1; then
    echo "setup: cmake is missing; install it (https://cmake.org) or Homebrew, then run again" >&2
    exit 1
  fi
  brew install cmake
fi

sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
# A model: where it goes, where it comes from, its SHA-256 (ADR 0029).
fetch() {
  if [ -f "$1" ] && [ "$(sum "$1")" = "$3" ]; then return; fi
  mkdir -p models
  echo "setup: downloading $1"
  curl -fL --retry 3 --progress-bar -o "$1.part" "$2"
  if [ "$(sum "$1.part")" != "$3" ]; then
    rm -f "$1.part"
    echo "setup: $1 does not match its SHA-256" >&2
    exit 1
  fi
  mv "$1.part" "$1"
}
fetch models/multilingual-e5-large-instruct-q8_0.gguf \
  https://huggingface.co/Ralriki/multilingual-e5-large-instruct-GGUF/resolve/main/multilingual-e5-large-instruct-q8_0.gguf \
  2fa4f519b83ecd31a35265b9dd43790bf0abd1ff942e7b982861ee674dc7bec8
fetch models/embeddinggemma-2-Q8_0.gguf \
  https://huggingface.co/ggml-org/embeddinggemma-2-GGUF/resolve/main/embeddinggemma-2-Q8_0.gguf \
  2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135
echo "setup: ready"

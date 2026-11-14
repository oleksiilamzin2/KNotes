#!/bin/sh
# Sets up a development machine (ADR 0028): the build tools Rust does not bring (cmake, for
# llama.cpp) and the files the build takes that git does not keep (the embedding model).
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

model=models/embeddinggemma-300M-Q8_0.gguf
url=https://huggingface.co/ggml-org/embeddinggemma-300M-GGUF/resolve/main/embeddinggemma-300M-Q8_0.gguf
sha=b5ce9d77a3fc4b3b39ccb5643c36777911cc4eb46a66962eadfa3f5f60490d63
sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
if [ ! -f "$model" ] || [ "$(sum "$model")" != "$sha" ]; then
  mkdir -p models
  echo "setup: downloading the embedding model (334 MB)"
  curl -fL --retry 3 --progress-bar -o "$model.part" "$url"
  if [ "$(sum "$model.part")" != "$sha" ]; then
    rm -f "$model.part"
    echo "setup: the model downloaded does not match its SHA-256" >&2
    exit 1
  fi
  mv "$model.part" "$model"
fi
echo "setup: ready"

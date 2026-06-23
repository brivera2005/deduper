# Contributing to Deduper

Thanks for your interest in making Deduper better!

## Getting started

1. Fork the repo and clone it locally.
2. Install [Node.js 18+](https://nodejs.org), [Rust stable](https://rustup.rs), and the Visual Studio 2022 C++ build tools (Windows).
3. Copy `.env.example` to `.env` (or `deduper-oauth.json.example` to `deduper-oauth.json`) for local OAuth — never commit either file. Release builds embed credentials from `.env` at compile time.
4. Run `npm install`, then `npm run tauri dev`.

## Pull requests

- Keep changes focused; one logical change per PR when possible.
- Match existing code style (Rust + React/TypeScript).
- Do not commit secrets, `config.json`, or database files.
- Describe what you tested (e.g. Drive scan, MTP detect, wizard flow).

## Reporting issues

Include OS version, Deduper version, and steps to reproduce. Redact OAuth client IDs/secrets and personal file paths.

## Code of conduct

Be respectful and constructive. Deduper is safety-first — contributions should preserve that default (no surprise deletes).
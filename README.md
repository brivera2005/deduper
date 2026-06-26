# Deduper

> **Free up Google storage — keep one safe copy on your PC.**

Deduper helps you find duplicate photos and videos across **Google Drive**, **Google Photos**, **Gmail attachments**, **your phone**, and **this PC**. It shows proof, saves missing files to a PC folder, and can move verified duplicates to **Google Drive Trash** — only with your explicit OK.

Built with **Tauri 2** (Rust + React). Windows desktop app with system tray and **automatic updates** from GitHub Releases.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Features (v1.0)

### Check everything (one button)
- **Google Drive** — online files in your Google account
- **Google Photos** — backed-up photo library
- **Gmail** — large email attachments (5 MB+)
- **This PC** — your photo folder
- **Your phone** — Android over USB

### Safety first
- **Never auto-deletes** — you confirm every cleanup
- **Proof panel** — sample files with paths on your PC
- **Verified copies** — re-checks every file after saving
- **Activity log** + **HTML/JSON report** export (print to PDF)

### Free up space
- **Google storage meter** — see used vs free (Drive + Photos + Gmail share one pool)
- **Move to Google Drive Trash** — only verified duplicates; type `MOVE TO TRASH` to confirm
- **Preview cleanup** before anything changes

### Install & updates
- **Signed Windows installer** (`.exe`) from [Releases](https://github.com/brivera2005/deduper/releases)
- **Auto-update** — after the first install, Deduper checks GitHub for new versions on launch (tray menu → **Check for updates** too)
- Updates are **not** via Windows Update or the Microsoft Store — the app downloads signed updates from GitHub when you approve

### Speed
- Parallel hashing on PC/phone
- Incremental scans skip unchanged files

---

## Quick start (for your dad)

1. Install **Deduper_*_x64-setup.exe** from [Releases](https://github.com/brivera2005/deduper/releases).
2. Complete **Setup** — pick PC photo folder → connect Google → plug in phone (optional).
3. Click **Check all my photos & videos**.
4. Review results and **Proof** section.
5. **Save to my PC folder** for files only online.
6. **Enable Google Drive cleanup** → preview → **Move to Trash** when ready.
7. **Save report** for a receipt you can print.

Future versions install from inside the app when an update is available.

---

## Google Cloud setup (developers)

Enable these APIs for your OAuth app:
- Google Drive API
- Google Photos Library API
- Gmail API

OAuth redirect: `http://127.0.0.1:8888/oauth/callback`

See `.env.example` for credentials. For **OAuth verification** (remove “unverified app” warning), see [docs/GOOGLE_OAUTH_VERIFICATION.md](docs/GOOGLE_OAUTH_VERIFICATION.md) and [docs/PRIVACY.md](docs/PRIVACY.md).

---

## Build & release (developers)

```powershell
npm install
npm run tauri dev    # development
npm run tauri build  # Windows installer + updater artifacts
```

Requires Node.js 18+, Rust (rustup.rs), VS Build Tools with C++.

Release pipeline: push a `v*` tag → GitHub Actions builds signed installer. See [docs/RELEASE.md](docs/RELEASE.md) for signing secrets setup.

After pulling, run `cargo build` once in `src-tauri/` to refresh `Cargo.lock`.

---

## License

[MIT](LICENSE)

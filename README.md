# Deduper

> **Free up Google storage — keep one safe copy on your PC.**

Deduper helps you find duplicate photos and videos across **Google Drive**, **Google Photos**, **Gmail attachments**, **your phone**, and **this PC**. It shows proof, saves missing files to a PC folder, and can move verified duplicates to **Google Drive Trash** — only with your explicit OK.

Built with **Tauri 2** (Rust + React). Windows desktop app with system tray.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Features (v0.3)

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

### Free up space (Phase 2)
- **Google storage meter** — see used vs free (Drive + Photos + Gmail share one pool)
- **Move to Google Drive Trash** — only verified duplicates; type `MOVE TO TRASH` to confirm
- **Preview cleanup** before anything changes

### Speed
- Parallel hashing on PC/phone
- Incremental scans skip unchanged files

---

## Quick start (for your dad)

1. Install from [Releases](https://github.com/brivera2005/deduper/releases) or build from source (below).
2. Complete **Setup** — pick PC photo folder → connect Google → plug in phone (optional).
3. Click **Check all my photos & videos**.
4. Review results and **Proof** section.
5. **Save to my PC folder** for files only online.
6. **Enable Google Drive cleanup** → preview → **Move to Trash** when ready.
7. **Save report** for a receipt you can print.

---

## Google Cloud setup (developers)

Enable these APIs for your OAuth app:
- Google Drive API
- Google Photos Library API
- Gmail API

OAuth redirect: `http://127.0.0.1:8888/oauth/callback`

See `.env.example` for credentials.

---

## Build from source

```powershell
npm install
npm run tauri dev    # development
npm run tauri build  # Windows installer
```

Requires Node.js 18+, Rust (rustup.rs), VS Build Tools with C++.

After pulling, run `cargo build` once in `src-tauri/` to refresh `Cargo.lock`.

---

## License

[MIT](LICENSE)

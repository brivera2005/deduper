# Deduper

> **Tagline:** *Same photo, three places? Deduper finds the copies — and never deletes anything for you.*

**Safe media deduplication** across your Android phone, Google Drive, and local PC folders. Find duplicates with confidence levels, copy unique files to a vault, and get a report of Drive space you can recover — **never auto-deletes**.

Built with **Tauri 2** (Rust backend + React UI). Runs in the Windows system tray.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Features

- **7-step setup wizard** — vault folder, Google Drive, Android USB, first-scan tips
- **Cross-source inventory** — local folders, Drive metadata (`md5Checksum`), Android MTP over USB
- **SHA-256 hashing** for local and phone files; resumable scan checkpoints in SQLite
- **Duplicate groups + recovery report** — see how much Drive space you could reclaim
- **Copy uniques to vault** with dry-run before any copy
- **Audit log** of actions
- **System tray** — minimize on close; USB plug notifications

---

## Screenshots

<!-- Add PNGs under docs/screenshots/ and link them here before the next release -->

| Dashboard | Setup wizard |
|-----------|--------------|
| *Coming soon* | *Coming soon* |

---

## Install from a release

1. Open [Releases](https://github.com/brivera2005/deduper/releases) on GitHub.
2. Download the latest **Windows x64** installer (`Deduper_*_x64-setup.exe`).
3. Run the installer (or silent install: `Deduper_0.1.0_x64-setup.exe /S`).
4. Launch **Deduper** from the Start menu and complete the setup wizard.

> Releases will be published as the project matures. Until then, [build from source](#build-from-source) below.

---

## Quick start (dad-friendly)

1. **Install Rust** (one time): [https://rustup.rs](https://rustup.rs) — defaults are fine.
2. **Install Node.js** (one time): [https://nodejs.org](https://nodejs.org) LTS.
3. **Build and install** (or run from source):

   ```powershell
   npm install
   npm run tauri build
   # Silent install:
   .\src-tauri\target\release\bundle\nsis\Deduper_0.1.0_x64-setup.exe /S
   ```

4. **First launch** opens a **7-step setup wizard**:
   - Welcome
   - How it works (never auto-delete)
   - Pick PC vault folder
   - Connect Google Drive (OAuth credentials + sign in)
   - Connect Android phone (USB + detect)
   - First scan tips (overnight OK)
   - Done summary

5. Check the hero metric: **Drive space you can recover**.

> **Nothing gets deleted automatically.** Review the recovery report first.

Use **Run setup again** anytime from the Setup button in the header.

---

## Setup wizard overview

| Step | What you do |
|------|-------------|
| 1 | Welcome and safety promise |
| 2 | How dedup works (report-first, optional copy to vault) |
| 3 | Choose a **vault folder** on your PC for consolidated copies |
| 4 | Paste **Google OAuth** desktop app credentials and connect Drive |
| 5 | Plug in Android, enable **File transfer**, detect phone |
| 6 | Tips for first full scan (can run overnight) |
| 7 | Summary and link to dashboard |

Wizard progress is stored in `%APPDATA%\com.deduper.app\config.json` (local only, not in git).

---

## Google Drive OAuth setup

Deduper uses **read-only** Drive access to list files and read `md5Checksum` metadata (no file downloads required for dedup).

### 1. Create Google Cloud project

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a project (e.g. "Deduper")
3. **APIs & Services → Library** → enable **Google Drive API**

### 2. Create OAuth credentials

1. **APIs & Services → Credentials → Create Credentials → OAuth client ID**
2. Application type: **Desktop app**
3. Name it "Deduper Desktop"
4. Copy the **Client ID** and **Client secret**

### 3. Configure redirect URI

Add this authorized redirect URI on the OAuth client:

```
http://127.0.0.1:8888/oauth/callback
```

### 4. Enter credentials in Deduper

**Installed app (recommended):** Paste Client ID + Secret in the setup wizard (Step 4) or Settings. Saved locally to:

```
%APPDATA%\com.deduper.app\config.json
```

**Development:** Copy `.env.example` to `.env` in the project root:

```env
GOOGLE_CLIENT_ID=your-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-secret
```

Never commit `.env` or `config.json`.

### 5. Connect in the app

Click **Connect Google Drive** → browser opens → sign in → approve read-only access → return to Deduper.

---

## Android phone (USB)

1. Plug your Android phone into the PC with a USB cable.
2. Unlock the phone.
3. On the USB notification, choose **File transfer** / **Transfer files** (not "Charge only").
4. In Deduper, click **Detect phone** (wizard) or **Connect Phone** (dashboard).
5. Click **Scan Phone** — DCIM, Pictures, Download, etc. are hashed over USB (slow; progress shown).

If USB detection fails, use **Manual import** to pick a folder copied from the phone.

Deduper polls for new phones every 8 seconds and can show a tray notification: *"Phone detected — open Deduper to scan your photos."*

---

## Build from source

### Prerequisites (Windows)

- **Node.js** 18+ — [nodejs.org](https://nodejs.org)
- **Rust** — [rustup.rs](https://rustup.rs) (stable)
- **Visual Studio Build Tools 2022** with **Desktop development with C++**

  ```powershell
  winget install Microsoft.VisualStudio.2022.BuildTools
  ```

### Commands

```powershell
npm install
npm run dev          # Vite only (browser)
npm run tauri dev    # Full desktop app + hot reload
npm run tauri build  # Production .exe installer
```

**Data location (runtime, not in repo):**

| File | Purpose |
|------|---------|
| `%APPDATA%\com.deduper.app\deduper.db` | SQLite inventory + audit log |
| `%APPDATA%\com.deduper.app\config.json` | OAuth credentials, vault path, wizard status |

---

## Architecture

```
Phone (MTP/USB) ──┐
Local folders  ───┼──► Scanner engine ──► SQLite inventory ──► Duplicate groups
Google Drive   ───┘         │                                      │
                              └── SHA-256 / md5 metadata              └── Recovery report + vault copy
```

| Component | Status |
|-----------|--------|
| 7-step setup wizard | Working |
| SQLite inventory + resumable scan checkpoints | Working |
| SHA-256 local hashing | Working |
| Google Drive metadata scan (`md5Checksum`) | Working |
| OAuth config in AppData (installed exe) | Working |
| Token refresh | Working |
| Local folder scanner | Working |
| Android USB MTP detect + scan (Windows WPD/Shell) | Working |
| Manual phone import (folder picker) | Working |
| USB plug detection + tray notification | Working |
| Duplicate grouping + recovery report | Working |
| Copy uniques to vault (dry-run supported) | Working |
| Audit log | Working |
| System tray (show/quit, minimize on close) | Working |
| Move to Drive Trash (write OAuth) | Planned |
| Immich integration | Planned v2 |

### Confidence levels

- **verified_duplicate** — same content hash (SHA-256 locally; md5 prefix for Drive) in 2+ places
- **unique** — no matching hash elsewhere
- **unknown** — not yet hashed or Drive file without checksum

### Safety defaults

- Destructive actions support **dry-run**
- Drive cleanup is **report-only** by default
- Optional trash move will require `drive.file` scope + confirmation (future)

---

## Roadmap

- **Drive trash move** — optional, confirmed moves to Google Drive trash (never silent delete)
- **Immich** — scan and dedupe against a self-hosted Immich library
- **macOS / Linux** — Tauri ports after Windows MVP is stable
- **Release builds + auto-update** — signed installers on GitHub Releases
- **Screenshot gallery + demo video** in README

---

## Project structure

```
deduper/
├── src/                 # React UI (dashboard, setup wizard)
├── src/components/      # SetupWizard.tsx
├── src-tauri/
│   ├── src/
│   │   ├── config/      # AppData config.json (OAuth, wizard, vault)
│   │   ├── db/          # SQLite schema + migrations
│   │   ├── hash/        # SHA-256 engine
│   │   ├── scanner/     # local, drive, mtp (PowerShell/WPD)
│   │   ├── oauth/       # Google Drive PKCE OAuth
│   │   ├── audit/       # Audit log
│   │   ├── watcher/     # USB device polling + notifications
│   │   └── commands/    # Tauri IPC commands
│   └── tauri.conf.json
├── CONTRIBUTING.md
├── LICENSE
└── README.md
```

---

## Testing with your Drive account

1. Complete OAuth setup above
2. Run setup wizard — vault, Drive, phone
3. **Scan Drive** → **Scan Phone** → **Scan** local folder
4. Hero metric shows recoverable GB/count
5. Use **Dry-run copy** before copying uniques to vault

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Bug reports and PRs welcome.

## License

[MIT](LICENSE) — use freely, attribution appreciated.
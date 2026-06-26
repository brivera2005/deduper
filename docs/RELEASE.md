# Release & signing setup

Deduper v1.0 ships with:

1. **Windows NSIS installer** (`.exe`) — built on GitHub Actions when you push a `v*` tag
2. **Tauri auto-updater** — signed update packages + `latest.json` on GitHub Releases
3. **Optional Windows Authenticode** — SmartScreen trust for the installer (requires a code-signing certificate)

Auto-updates use **GitHub Releases**, not Windows Update or the Microsoft Store. Installed apps check for updates on launch and via the tray/header menu.

---

## One-time: Tauri updater signing key

A keypair was generated locally at:

- Private key: `%USERPROFILE%\.tauri\deduper.key` (**never commit**)
- Public key: embedded in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`

### GitHub repository secrets

Add these in **Settings → Secrets and variables → Actions**:

| Secret | Value |
|--------|--------|
| `TAURI_SIGNING_PRIVATE_KEY` | Full contents of `deduper.key` (the private key file) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password used when generating the key |

If you lose the private key or password, you cannot publish compatible updates — generate a new keypair and update the pubkey in `tauri.conf.json`.

---

## Optional: Windows Authenticode (SmartScreen)

Without this, the installer still works but Windows may show “Unknown publisher.”

1. Purchase an EV or standard code-signing certificate
2. Export as `.pfx`
3. Base64-encode the PFX and add secrets:

| Secret | Value |
|--------|--------|
| `WINDOWS_CERTIFICATE` | Base64 of `.pfx` file |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX password |

`tauri-action` passes these to the Windows signer automatically.

---

## Publishing a release

```powershell
# Bump version in package.json, Cargo.toml, tauri.conf.json first
git add -A
git commit -m "Release v1.0.1"
git tag v1.0.1
git push origin main
git push origin v1.0.1
```

The workflow `.github/workflows/release.yml` will:

- Build the Windows installer
- Sign update artifacts with your Tauri key
- Upload assets to a **draft** GitHub Release (includes `latest.json` for auto-update)
- Review the draft on GitHub, edit release notes, then **Publish**

---

## How auto-update works for users

1. App calls `check()` against  
   `https://github.com/brivera2005/deduper/releases/latest/download/latest.json`
2. If a newer signed version exists, the UI offers **Install update**
3. Download + install runs in the background; app restarts via `tauri-plugin-process`

Users must install **v1.0.0+ from a GitHub Release** once. After that, updates are in-app.

---

## Local build (developer)

```powershell
npm install
npm run tauri build
```

Output: `src-tauri/target/release/bundle/nsis/` and updater artifacts if `createUpdaterArtifacts` is true.

To test signing locally:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\deduper.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "your-password"
npm run tauri build
```

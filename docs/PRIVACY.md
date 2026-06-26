# Deduper Privacy Policy

**Last updated:** June 26, 2025  
**App:** Deduper (Windows desktop)  
**Publisher:** Benjamin Rivera  
**Contact:** [GitHub Issues](https://github.com/brivera2005/deduper/issues)

## Summary

Deduper runs on your PC. Your photos, videos, and file metadata stay on your machine unless you explicitly connect Google and choose actions that talk to Google (scan Drive, Photos, Gmail, or move files to Trash).

We do not operate a Deduper cloud service and we do not sell your data.

## What stays on your PC

- Scan results, duplicate groups, and activity logs (SQLite database in your Windows app data folder)
- Your chosen PC photo folder (“vault”) and copied files
- Google OAuth tokens (encrypted at rest by Windows; stored locally for reconnecting without signing in every time)
- Exported HTML/JSON proof reports in your vault’s `_deduper/receipts/` folder

## What goes to Google (only when you connect)

When you click **Connect Google** or enable cleanup, Deduper uses your OAuth consent to call Google APIs on your behalf:

| API | Purpose | Data sent |
|-----|---------|-----------|
| Google Drive | List and hash files; download copies; move duplicates to Trash (with your confirmation) | File metadata (name, size, MD5), file content when downloading |
| Google Photos Library | List backed-up photos/videos for duplicate detection | Photo metadata and content URLs for hashing |
| Gmail | Find large attachments for duplicate detection | Message metadata and attachment metadata (not email body text for reading) |

Deduper does not send your files to any server other than Google’s APIs when you use those features.

## What we do not collect

- No analytics or crash reporting SDKs in the app
- No account system on our servers
- No upload of your media to Deduper-owned infrastructure

## Auto-updates

When you install from GitHub Releases, Deduper may check  
`https://github.com/brivera2005/deduper/releases/latest/download/latest.json`  
for newer versions. Update packages are cryptographically signed (Tauri updater). Installing an update downloads the installer from GitHub Releases — not Windows Update or the Microsoft Store.

## Third parties

- **Google** — OAuth and APIs when you connect (subject to [Google Privacy Policy](https://policies.google.com/privacy))
- **GitHub** — hosting releases and update manifests

## Your choices

- Disconnect Google anytime in the app
- Revoke Deduper’s access in [Google Account → Security → Third-party access](https://myaccount.google.com/permissions)
- Uninstall Deduper to remove local app data (Windows Settings → Apps)

## Children

Deduper is a general household utility and is not directed at children under 13.

## Changes

We may update this policy on GitHub. Material changes will be noted in release notes.

## OAuth verification

For Google Cloud OAuth verification requirements, see [GOOGLE_OAUTH_VERIFICATION.md](./GOOGLE_OAUTH_VERIFICATION.md).

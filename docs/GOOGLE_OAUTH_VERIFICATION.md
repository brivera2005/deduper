# Google OAuth Verification Guide (Deduper)

Use this checklist to move Deduper from **Testing** to **Published / Verified** in Google Cloud Console so anyone can sign in without the “unverified app” warning.

## 1. Google Cloud project

1. Open [Google Cloud Console](https://console.cloud.google.com/)
2. Create or select project **Deduper**
3. Enable APIs:
   - Google Drive API
   - Google Photos Library API
   - Gmail API

## 2. OAuth consent screen

**User type:** External  
**App name:** Deduper  
**User support email:** your email  
**Developer contact:** your email  

**App home page:**  
`https://github.com/brivera2005/deduper`

**Privacy policy URL:**  
`https://github.com/brivera2005/deduper/blob/main/docs/PRIVACY.md`

**Terms of service (optional but recommended):**  
Same repo or a simple GitHub Pages site.

### Scopes (justify each in the verification form)

| Scope | Why Deduper needs it |
|-------|----------------------|
| `https://www.googleapis.com/auth/drive.readonly` | Read Drive file list and MD5 hashes to find duplicates vs your PC |
| `https://www.googleapis.com/auth/drive` | Move **verified** duplicate files to Drive Trash only after you type `MOVE TO TRASH` |
| `https://www.googleapis.com/auth/photoslibrary.readonly` | Compare Google Photos library against PC/phone copies |
| `https://www.googleapis.com/auth/gmail.readonly` | Find large attachments that duplicate files you already have locally |

**Sensitive scope note:** Full `drive` scope is required only for Trash cleanup. The app requests readonly scopes first at connect; cleanup is a separate explicit step.

### Authorized redirect URIs

```
http://127.0.0.1:8888/oauth/callback
```

Deduper uses a local loopback redirect (desktop app pattern). Add this exact URI under **OAuth 2.0 Client ID → Web application** (or Desktop if you use desktop client type — match what the app embeds at build time).

## 3. OAuth client credentials

Build-time credentials (see `.env.example`):

- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`

For production releases, embed credentials in CI secrets or `src-tauri/resources/oauth.defaults.json` at build time (never commit secrets to git).

## 4. Verification submission

1. Complete all consent screen fields (logo 120×120 PNG recommended)
2. Add test users while in Testing mode
3. When ready: **Publish app** → **Prepare for verification**
4. Provide:
   - YouTube demo (2–3 min): connect Google → run check → show proof → optional Trash preview (do not trash real data on demo account)
   - Written explanation per sensitive scope (use table above)
   - Link to privacy policy
   - Explanation that data stays local; Google APIs used only on user action

Review typically takes several days to weeks.

## 5. Branding (removes “unverified” after approval)

After verification, Google shows your app name and logo on the consent screen instead of “This app isn’t verified.”

## 6. Testing before verification

- Keep app in **Testing** with up to 100 test Google accounts (fine for family use)
- Test users must be added manually in Cloud Console

## 7. Security practices (already in Deduper)

- Local OAuth token storage in app data directory
- No server-side token storage
- Trash requires typed confirmation `MOVE TO TRASH`
- Dry-run preview before any destructive action
- Audit log of all actions

## Questions Google often asks

**Does your app need all requested scopes?**  
Yes — readonly for audit; full Drive only for user-confirmed Trash moves.

**Where is user data stored?**  
On the user’s Windows PC only.

**Do you share data with third parties?**  
No — except calls to Google APIs the user initiates.

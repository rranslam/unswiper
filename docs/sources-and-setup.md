# Media sources — configuration and setup

The app supports **either or both**:

1. **Local tree** — process a folder tree of photos/videos on disk  
2. **Google API** — OAuth to the user’s Google account and index media via Google APIs  

MVP recommendation: ship **local tree** first; add Google as a second adapter.

---

## Config locations

| OS | Directory |
|----|-----------|
| Linux | `~/.config/visit-media-reduce/` |
| macOS | `~/Library/Application Support/Unswiper/` |

Primary file: `config.yaml` (JSON also acceptable).  
Examples in repo: [`config/examples/`](../config/examples/).

Secrets (OAuth client secret, refresh tokens) go in:

- `credentials.json` (Google desktop client download), and  
- OS keychain / `tokens.json` with restrictive permissions (`0600`) — **not** in git.

---

## Option A — Local tree

### When to use

- Photos already on disk (`~/Pictures`, external drive, NAS mount).  
- Google **Takeout** unzipped locally.  
- Synced folders (e.g. Drive for desktop) that appear as a normal directory tree.

### Setup

1. Install/run the desktop app.  
2. Set one or more roots in config (or first-run folder picker).  
3. Run Index → groups appear → reduce as usual.  
4. Choose commit mode: system trash **or** move to a quarantine folder.

### Example

See `config/examples/config.local-tree.yaml`.

### Commit semantics (local)

| Mode | Behavior |
|------|----------|
| `trash` | Move rejects to OS trash (recoverable) |
| `quarantine` | Move to `<root>/_Rejected/Unswiper/<date>/` |
| `dry_run` | Record decisions only; do not move files |

Never hard-delete without an explicit settings toggle (default off for MVP).

### Useful knobs

- `include_globs` / `exclude_globs` (skip `node_modules`, `@eaDir`, `.Trash`, etc.)  
- `follow_symlinks` (default false)  
- `min_file_bytes` / video extensions list  

---

## Option B — Google API

### When to use

- Library lives in Google Photos / Google Drive and you want index + review without a full Takeout.  
- You accept that **delete/trash via API may be limited**; the app must still allow local decision tracking and export.

### Google Cloud setup (once per developer/distributor)

1. Open [Google Cloud Console](https://console.cloud.google.com/).  
2. Create a project (e.g. `visit-media-reduce`).  
3. Enable APIs you will call (start with what you need):
   - **Photos Library API** — media items / albums (verify current scopes and library access rules; they change over time).  
   - **Google Drive API** — if media is in Drive folders instead of / in addition to Photos.  
4. **OAuth consent screen**
   - User type: External (or Internal for Workspace-only).  
   - App name, support email, scopes you request.  
   - Add test users while in Testing.  
5. **Credentials → Create credentials → OAuth client ID**
   - Application type: **Desktop app**  
   - Download JSON → save as `credentials.json` in the app config directory.  
6. First app run: browser opens; user consents; refresh token stored locally.

### Scopes (illustrative — verify against current Google docs)

Prefer least privilege:

- Read/index: photoslibrary.readonly and/or Drive readonly  
- Mutating (only if you implement and Google allows for your use case): narrower write scopes  

Document actual scopes in the consent screen and in-app “Google connection” settings.

### API reality check (design constraint)

Google Photos Library API is **not** a full “manage my entire camera roll like a local disk” API for every operation. Design the Google adapter with:

```text
capabilities.canDelete = false | true   // detected or configured
```

If delete isn’t available:

- Keep/reject still work **locally** in SQLite.  
- Commit offers: export reject list, open item URLs, or instruct user to finish in Google Photos.  
- Optional later: “download originals then treat as local tree.”

### Example

See `config/examples/config.google.yaml` and `config/examples/config.both.yaml`.

### Token storage

- Store refresh token in OS keychain when possible.  
- Fallback: `tokens.json` mode `0600`.  
- Provide **Disconnect Google** that deletes local tokens.

---

## Option C — Both

Enable both sources in one profile. Groups may be built:

- **per source** (simpler MVP), or  
- **merged** by time/geo (later; watch duplicates).

MVP: **separate library profiles** or separate group lists per source to avoid false merges.

---

## First-run UX (setup wizard)

1. Choose source: **Folder on this computer** / **Google account** / **Both**.  
2. Local: folder picker → validate readable → optional exclude patterns.  
3. Google: “Sign in with Google” → OAuth → show connected email + capabilities.  
4. Set default keep target and commit mode.  
5. Start Index (progress: files seen / API pages).  

Config file remains the source of truth for power users and headless re-index later.

---

## Security checklist

- [ ] OAuth client secret not committed to the project repo  
- [ ] Config examples use placeholders only  
- [ ] Token file permissions restricted  
- [ ] HTTPS only to Google endpoints  
- [ ] User can wipe index DB and tokens without uninstalling  

---

## Verify setup

**Local**

```bash
# After app CLI exists (planned):
visit-media-reduce index --source local
visit-media-reduce groups --limit 5
```

**Google**

```bash
visit-media-reduce auth google
visit-media-reduce index --source google
```

Until CLI exists: use the desktop Setup screen and confirm SQLite row counts in the app’s debug/about panel.

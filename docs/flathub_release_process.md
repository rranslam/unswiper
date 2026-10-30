<!-- SPDX-License-Identifier: MIT -->
# Flathub release process

How Unswiper gets onto Flathub (once) and how each release reaches it afterwards (on Rob's "go").
Sources, read 30 Oct 2026:
[Submission](https://docs.flathub.org/docs/for-app-authors/submission) ·
[Requirements](https://docs.flathub.org/docs/for-app-authors/requirements) ·
[Linter](https://docs.flathub.org/linter) ·
[MetaInfo guidelines](https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/) ·
[Verification](https://docs.flathub.org/docs/for-app-authors/verification) ·
[Maintenance](https://docs.flathub.org/docs/for-app-authors/maintenance)

> **Read first: Flathub's Generative AI policy** ([requirements](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)).
> - The **Flathub manifest must not contain AI-generated or AI-assisted content.** Rob writes the
>   manifest in the Flathub submission himself. This repo's `packaging/flatpak` manifest is only a reference.
> - **AI tools must not open or automate the submission PR**, or write its commit messages, description or review replies.
>   Rob does all of Part A by hand.
> - **Disclose** any AI-generated code or docs in the app itself, naming the parts and roughly how much.
> - The update workflow in Part B is a fixed script with no AI in it. It only changes the tag, commit and
>   `cargo-sources.json`, the same edits Flathub's own bot makes.

## Requirements checklist

Status: ✅ met · 🔧 fixed on branch `feat/flathub` · ⏳ open (owner in brackets)

**Inclusion**
- ✅ A graphical desktop app with real functionality: it is not a console tool or a thin wrapper.
- ⏳ [Rob] **Development history.** Flathub wants "a meaningful history" and tagged releases. Apps that have
  existed only a short time "will generally not be accepted". Unswiper's first commit is 11 Oct 2026, and
  it has about 200 commits and 0.1.0–0.6.4 tags. Expect reviewers to raise this. Waiting a few
  weeks or months of steady releases helps.
- ⏳ [Rob] AI disclosure (see above).
- ✅ Stable releases only: tagged `vX.Y.Z` releases, no nightlies.
- ✅ No upstream Flatpak would conflict: Rob is upstream, so his submission is the official one.

**App ID** ([rules](https://docs.flathub.org/docs/for-app-authors/requirements#application-id))
- ✅ `io.github.rranslam.Unswiper`: 4 components, the `io.github.` prefix for a GitHub-hosted app, and no `.desktop`/`.app` ending.
- ✅ Matches `<id>` in the metainfo, and the derived repo URL `https://github.com/rranslam/unswiper` is reachable.
- ⏳ [Rob] Verify after acceptance: **flathub.org → Developer Portal → Unswiper → Verification**, signed in
  with GitHub as the owner of `rranslam/unswiper`. No website or DNS token is needed.

**Manifest**
- ✅ Builds offline. Crates are vendored in `cargo-sources.json` by the pinned `flatpak-cargo-generator.py`, and the build uses `CARGO_NET_OFFLINE`
  and `cargo --offline`. No `--share=network` in build-args.
- ✅ Built entirely from source (libde265, libheif and the app). No binaries.
- ✅ Every dependency is an archive with a `sha256`.
- ⏳ [Rob, in the Flathub manifest] **The source must be a git tag + commit, not `type: dir`.** Use
  `type: git`, `url: https://github.com/rranslam/unswiper.git`, `tag: vX.Y.Z`, `commit: <40-char sha>`.
  Put `url`, `tag` and `commit` on consecutive lines in that order; `update-flathub-manifest.sh` relies on it.
  Add `x-checker-data` only if Part B is not used (see below).
- ⏳ [Rob] **Latest runtime at submission.** The manifest uses GNOME `50`. The linter warns
  `runtime-update-available-to-org.gnome.Platform-51`. Bump to `51` with `rust-stable//26.08` right before
  submitting, and re-test HEIC and video thumbnails (the codec notes in `packaging/flatpak/README.md` were checked on 50).
- ✅ The manifest file in the Flathub repo is named `io.github.rranslam.Unswiper.yml` and sits at the top level.
- 🔧 Logical key order (style guide): `cleanup` now comes before `build-options`.
- 🔧 License installed to `/app/share/licenses/io.github.rranslam.Unswiper/` (flatpak-builder auto-installs the libheif and libde265 `COPYING` files).
- ✅ Both arches (x86_64 and aarch64) build, so no `flathub.json` is needed.
- ⏳ [Rob, in the PR text] **Justify the permissions.** Reviewers judge them separately, and a portal is
  mandatory where one fits:
  - `--share=ipc`, `--socket=wayland`, `--socket=fallback-x11`, `--device=dri`: standard for a GTK 4 app.
    `dri` is for GL rendering of thumbnails and video.
  - `--filesystem=xdg-pictures`, `--filesystem=xdg-videos`: library folders are picked with the
    FileChooser portal. Direct access lets the live folder watch use inotify on real paths (the document portal
    has no inotify). It also lets commits and Restore move files and their sidecars as a group, and lets the
    Google Photos import check free space at its default location. Expect pushback, so be ready to explain why the
    document portal alone is not enough.
  - `--filesystem=xdg-data/Trash`: Restore Last Commit has to put files back from the freedesktop Trash.
    The trash portal can only trash files, it cannot restore them.

**Metainfo, desktop file, icons**
- ✅ Validates: `appstreamcli validate` in CI, and `flatpak-builder-lint appstream` passed (pedantic only).
- ✅ `metadata_license` and `project_license` MIT (matching `Cargo.toml` and `LICENSE`), plus name, summary, description, developer `id` + name,
  launchable, homepage, bugtracker, branding colours, `<content_rating type="oars-1.1"/>` (no objectionable content), and dated
  `<release>` entries with notes.
- 🔧 Added `<url type="vcs-browser">`.
- ⏳ [Rob] **Screenshot.** The current one (1280×800, synthetic coloured "IMG n" tiles, so no real photos) is
  from 0.2.0 and has black space around the window. For the [quality guidelines](https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/quality-guidelines),
  take new screenshots of the current UI from **synthetic test data only** (e.g. the `docs/stress` generator or coloured test images),
  window only with no desktop around it, and link them by **tag** (`.../unswiper/vX.Y.Z/...`) rather than `main`.
- ✅ Desktop file passes `desktop-file-validate`, and `Icon=` matches the app ID.
- ✅ Icons: scalable SVG + symbolic + 16–512 PNGs under `hicolor`, named after the app ID.

**Linter** ([docs](https://docs.flathub.org/linter)). Run before submitting, and after every manifest change:
```bash
flatpak install --user -y flathub org.flatpak.Builder
flatpak run --command=flathub-build org.flatpak.Builder --install io.github.rranslam.Unswiper.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.rranslam.Unswiper.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
flatpak run io.github.rranslam.Unswiper
```
Result on 30 Oct 2026 (sonny, repo manifest): the only warning is `runtime-update-available-to-org.gnome.Platform-51`.
The `repo` check needs a full build, so run it with the Flathub manifest.

## Part A: first submission (one time, all by hand)

Every 👤 step is Rob's. Flathub's AI policy rules out automating them.

1. 👤 Decide on the history and AI-disclosure points above. Bump the runtime to 51, test, and release that version.
2. 👤 Take the new synthetic screenshots, update the metainfo `<screenshot>` URLs to the tag, and release.
3. 👤 On GitHub, fork **flathub/flathub** with **"Copy the master branch only" unchecked**. Then:
   `git clone --branch=new-pr git@github.com:rranslam/flathub.git && cd flathub && git checkout -b add-unswiper new-pr`
4. 👤 Write `io.github.rranslam.Unswiper.yml` at the top level, using `packaging/flatpak/` as a reference.
   It needs a git tag + commit source, the latest runtime, and no `dir` source. Add `cargo-sources.json`
   (`packaging/flathub/update-flathub-manifest.sh X.Y.Z .` writes it and fills in the tag/commit).
   Don't add any metainfo, desktop file or icons; they come from the source.
5. 👤 Build, run and lint locally (the commands above) until it is clean.
6. 👤 Commit, push, and open the PR on github.com **against the `new-pr` base branch** (never `master`), titled
   **`Add io.github.rranslam.Unswiper`**. Keep the PR template, and explain the permissions and the AI disclosure.
7. 👤 Answer the reviewers yourself. Never close the PR to make changes. Comment `bot, build` for a test build when asked.
8. 👤 After it is merged: turn on GitHub 2FA and **accept the invite** to `flathub/io.github.rranslam.Unswiper` within a week.
9. 👤 Sign in to flathub.org with GitHub, then **Developer Portal → Unswiper → Verification**.
10. 👤 Turn on Part B (below).

## Part B: every release (automated, Rob approves)

`.github/workflows/flathub_update.yml` runs when **Release** succeeds on a `v*` tag, or by hand
(Actions → Flathub update → Run workflow → version). It:
1. waits for Rob's approval on the **`flathub` environment** (this is the "go");
2. checks out the Flathub app repo with `FLATHUB_TOKEN`;
3. runs `packaging/flathub/update-flathub-manifest.sh X.Y.Z`. That sets `tag: vX.Y.Z` and the commit, regenerates
   `cargo-sources.json` from the tag's `Cargo.lock`, and fails if the metainfo has no `<release version="X.Y.Z">`;
4. lints the manifest, then opens the PR **"Update to X.Y.Z"** on `flathub/io.github.rranslam.Unswiper`.

Flathub then test-builds the PR and posts an install link. Rob installs the test build, checks it, and **merges**.
The official build is usually published 1–2 hours later. Releases built this way update the tag only. Changes to
permissions or the runtime still go in a PR Rob writes by hand (permission changes are held for moderation).

Because this workflow opens the update PRs, add `"disable-external-data-checker": true` to `flathub.json` in the
Flathub repo, or leave out `x-checker-data`, so that Flathub's bot doesn't open duplicate PRs.
(The other option is to drop Part B and use `x-checker-data` with `type: git` + `tag-pattern: ^v([\d.]+)$`. Then
flathub-bot opens the PR within about 2 hours of a tag, but it does not regenerate `cargo-sources.json`, so builds break whenever `Cargo.lock` changes.)

**Turn it on (Rob, once, after acceptance):**
1. Create a **fine-grained PAT** (GitHub → Settings → Developer settings → Fine-grained tokens) with resource owner
   `flathub`, only the repository `flathub/io.github.rranslam.Unswiper`, permissions **Contents: Read and write**
   and **Pull requests: Read and write**, and a 1-year expiry. If the flathub org blocks fine-grained tokens, use a
   classic token with `public_repo`. Rob creates it himself, and it never goes in a file or chat.
2. In `rranslam/unswiper` → Settings → Secrets and variables → Actions: add the secret **`FLATHUB_TOKEN`**, and the
   variable **`FLATHUB_ENABLED`** = `true`.
3. In Settings → Environments, create **`flathub`** with **Required reviewers: rranslam**.

Until `FLATHUB_ENABLED` is `true`, the workflow is skipped, and nothing changes for releases or the existing Flatpak CI.

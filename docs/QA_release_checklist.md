# QA release checklist

Work through this list for every release, before you push the `vX.Y.Z` tag. The tag's source archive and the release builds come from the tagged commit, so **every doc and version update must be merged into `main` BEFORE tagging**. Anything merged afterwards isn't in that release's source.

## 1. Code and CI

- [ ] The release branch is merged into `main` with a merge commit (no squash).
- [ ] All checks pass on the merge head. These are the jobs in `.github/workflows/`:
  - `rust.yml`: `lint` (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`), `test-linux` (`xvfb-run -a cargo test --workspace`), `test-windows`, `coverage`
  - `ci.yml`: `test-macos` (`swift test` with the blur helper)
  - `shellcheck.yml`: `shellcheck` (every tracked `*.sh`; runs only when a script or the workflow changes)
  - `appimage.yml`: `appimage` for x86_64 and aarch64
  - `flatpak.yml`: `validate` (metainfo and desktop file) and `flatpak` for x86_64 and aarch64
- [ ] Every open PR meant for this release (docs, README, Help) is merged into `main` first.

## 2. Version bump

- [ ] `Cargo.toml` `[workspace.package] version` is the new version.
- [ ] `Cargo.lock`: every workspace crate (`unswiper-*`) is at the new version (`cargo metadata --locked` succeeds).
- [ ] `CHANGELOG.md`: `## [Unreleased]` is renamed to `## [X.Y.Z] - YYYY-MM-DD`.
- [ ] `crates/unswiper-gtk/data/io.github.rranslam.Unswiper.metainfo.xml` has a new `<release version="X.Y.Z" date="…">` entry at the top.
- [ ] `apps/macos/scripts/build-dmg.sh`: the default `VERSION` is the new version.

## 3. Cross-check release versions in all docs

Set `OLD` to the previous version and `NEW` to the new one, then run:

```sh
OLD=0.5.0; NEW=0.6.0
O=${OLD//./\\.}; N=${NEW//./\\.}
# Anything still naming the previous version (review each hit: history and roadmap lines may stay)
rg -n "v?$O\b|v${O%\\.*}\b" --glob '!CHANGELOG.md' --glob '!Cargo.lock' --glob '!target/**' .
# The new version must appear in each of these files
for f in README.md CHANGELOG.md Cargo.toml Cargo.lock \
         crates/unswiper-gtk/data/io.github.rranslam.Unswiper.metainfo.xml \
         apps/macos/scripts/build-dmg.sh \
         apps/macos/Sources/Unswiper/Resources/Help/08-install-update.md; do
  rg -q "$N" "$f" && echo "ok      $f" || echo "MISSING $f"
done
```

Check each of these:

- [ ] `README.md`: the status line, the example `.dmg` name and the mounted-disk name in the install steps, the build example (`build-dmg.sh X.Y.Z`, the tag example), the feature list (new features added) and the roadmap.
- [ ] `CHANGELOG.md`: the new section has the correct date.
- [ ] `Cargo.toml` and `Cargo.lock` (see section 2).
- [ ] The metainfo.xml release entry.
- [ ] The default in `apps/macos/scripts/build-dmg.sh`.
- [ ] macOS in-app Help: `apps/macos/Sources/Unswiper/Resources/Help/08-install-update.md` (the example `.dmg` name), plus any other Help page that mentions a version or a new feature.
- [ ] `docs/`: plans and roadmap status for the new version. Leave historical decisions as they are.
- [ ] `STATUS.md`: the current direction.

## 4. Tag and release

- [ ] Create an annotated tag `vX.Y.Z` on the `main` merge commit and push it. Create only the tag; `.github/workflows/release.yml` creates the release.
- [ ] The Release workflow succeeds.
- [ ] The `.dmg` is ad-hoc signed only (no paid Apple Developer ID until Q1 2027), so the README and Help "open it the first time" steps still apply.
- [ ] The release has 5 assets: `Unswiper-X.Y.Z.dmg`, `Unswiper-x86_64.AppImage`, `Unswiper-aarch64.AppImage`, `Unswiper-x86_64.flatpak` and `Unswiper-aarch64.flatpak`.
- [ ] The release is marked **Latest**, and every `releases/latest/download/<asset>` link returns HTTP 200.
- [ ] The workflow generates the release notes automatically (`generate_release_notes: true`), which only lists PRs. Edit the release and paste the highlights from the new CHANGELOG section above them; keep the `**Full Changelog**: …/compare/vOLD...vNEW` line.

## 5. After release

- [ ] Add an empty `## [Unreleased]` section to `CHANGELOG.md` when the next change lands.
- [ ] Do a quick smoke test of the downloaded `.dmg`, AppImage and Flatpak on a copy of a small photo folder.
- [ ] Install each asset from the release page and check the version it reports (section 6).

## 6. Post-release install and version verification

Do this for **each of the 5 assets** after the Release workflow finishes. Download it from the release page (not a local build), install it the way a user would, launch it, and check that every place that shows a version says the tagged version `X.Y.Z`. If one doesn't, don't edit the release: open a `fix/version-report` PR with a test, then make a new patch release.

Replace `X.Y.Z` with the version (no `v`). Base URL: `https://github.com/rranslam/unswiper/releases/download/vX.Y.Z`.

**Linux AppImage** (`Unswiper-x86_64.AppImage` on x86_64, `Unswiper-aarch64.AppImage` on aarch64; check with `uname -m`):

```bash
mkdir -p /tmp/uv && cd /tmp/uv
curl -fLO https://github.com/rranslam/unswiper/releases/download/vX.Y.Z/Unswiper-$(uname -m).AppImage
chmod +x Unswiper-$(uname -m).AppImage
./Unswiper-$(uname -m).AppImage --version            # must print: unswiper X.Y.Z
./Unswiper-$(uname -m).AppImage --appimage-extract >/dev/null
grep -m1 'release version' squashfs-root/usr/share/metainfo/io.github.rranslam.Unswiper.metainfo.xml   # first entry: X.Y.Z
grep X-AppImage-Version squashfs-root/io.github.rranslam.Unswiper.desktop                              # X.Y.Z
./Unswiper-$(uname -m).AppImage                      # ☰ ▸ About Unswiper: the version badge shows X.Y.Z
cd / && rm -rf /tmp/uv
```

(No FUSE: add `--appimage-extract-and-run` before the other arguments.)

**Linux Flatpak** (`Unswiper-x86_64.flatpak` / `Unswiper-aarch64.flatpak`; user install, no sudo). First note whether it was already installed (`flatpak list --user --app | grep Unswiper`); if it was, skip the uninstall at the end.

```bash
mkdir -p /tmp/uv && cd /tmp/uv
curl -fLO https://github.com/rranslam/unswiper/releases/download/vX.Y.Z/Unswiper-$(uname -m).flatpak
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo   # for the GNOME runtime
flatpak install --user -y Unswiper-$(uname -m).flatpak
flatpak info --user io.github.rranslam.Unswiper | grep -i version     # Version: X.Y.Z
flatpak run io.github.rranslam.Unswiper --version                     # unswiper X.Y.Z
flatpak run io.github.rranslam.Unswiper                               # ☰ ▸ About Unswiper shows X.Y.Z
flatpak uninstall --user -y io.github.rranslam.Unswiper               # only if it wasn't installed before
cd / && rm -rf /tmp/uv
```

**macOS** (`Unswiper-X.Y.Z.dmg`; read it from the mounted image, don't replace the installed app):

```bash
cd /tmp && curl -fLO https://github.com/rranslam/unswiper/releases/download/vX.Y.Z/Unswiper-X.Y.Z.dmg
hdiutil attach -nobrowse -readonly Unswiper-X.Y.Z.dmg
/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' -c 'Print CFBundleVersion' \
  "/Volumes/Unswiper X.Y.Z/Unswiper.app/Contents/Info.plist"         # both X.Y.Z
open "/Volumes/Unswiper X.Y.Z/Unswiper.app"                          # Unswiper ▸ About Unswiper: "Version X.Y.Z"
# quit Unswiper, then:
hdiutil detach "/Volumes/Unswiper X.Y.Z" && rm /tmp/Unswiper-X.Y.Z.dmg
```

**No machine for an architecture?** Check the files without running them and mark the asset "static only, run test pending": for the AppImage, extract the squashfs (it starts at the `hsqs` offset; `unsquashfs -o <offset>`), then check the metainfo, the `.desktop` file and that `usr/bin/unswiper` contains `X.Y.Z`. For the Flatpak bundle, decompress its xz stream and look for the first `release version=` in the metainfo.

Before installing, check for older copies (see the README section *Uninstalling / removing old copies*): two Flatpaks (`--user` and `--system`) can make the app menu open the old one.

Record the result in the release PR or notes as a table: asset, machine, how checked (run / About / package metadata / static), version shown, pass or fail.

- [ ] `Unswiper-x86_64.AppImage`: `--version`, About and metadata show X.Y.Z
- [ ] `Unswiper-x86_64.flatpak`: `flatpak info`, `--version` and About show X.Y.Z
- [ ] `Unswiper-aarch64.AppImage`: same as x86_64
- [ ] `Unswiper-aarch64.flatpak`: same as x86_64
- [ ] `Unswiper-X.Y.Z.dmg`: Info.plist and About show X.Y.Z

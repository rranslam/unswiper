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

## 7. Flathub update (once Unswiper is on Flathub)

See [flathub_release_process.md](flathub_release_process.md) Part B.

- [ ] Before tagging: the metainfo has `<release version="X.Y.Z" date="…">` with notes (the workflow fails without it).
- [ ] After **Release** succeeds: approve the **`flathub`** environment gate on the *Flathub update* run ("go").
- [ ] The PR "Update to X.Y.Z" is open on `flathub/io.github.rranslam.Unswiper` and its `flathub/build` check is green.
- [ ] Install the test build from the bot's comment, start it, and check About shows X.Y.Z. Open a library of test images, then remove the `/test` ref.
- [ ] Merge the PR. Within a few hours, `flatpak update io.github.rranslam.Unswiper` from Flathub shows X.Y.Z.
- [ ] If the permissions or runtime changed, explain them in the PR (moderation hold).

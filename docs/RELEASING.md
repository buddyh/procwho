# Releasing procwho

1. Use a Mac with Xcode, Rust stable, Python 3, and the Developer ID Application
   certificate and private key in your keychain. The release script adds missing
   `aarch64-apple-darwin` and `x86_64-apple-darwin` Rust targets with `rustup target add`.
   It builds the CLI and Swift executable for both chips. No installation uses sudo.

2. Choose a notarytool keychain profile for `NOTARY_PROFILE`. Any existing profile
   for the Developer ID team can be reused. Check that a profile works with:

   ```sh
   xcrun notarytool history --keychain-profile <name>
   ```

   Only if none exists, run the interactive command below and enter your Apple
   account, team, and app-specific password when prompted. Keep those values out
   of shell history, repository files, and CI logs.

   ```sh
   xcrun notarytool store-credentials <name>
   ```

   See Apple's [notarytool workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

3. Update the workspace version in `Cargo.toml`, `Cargo.lock`, `app/VERSION`, and
   `CHANGELOG.md`. Check package metadata and the README install commands, including
   the archive file name in the Manual download block (the `shasum` and `tar` lines)
   and that the brew commands still match the tap. Confirm
   the supported-agent table against the detection rules. Commit all release work.
   The release script refuses tracked or untracked changes, including other agents'
   pending work. Do not stash or discard someone else's changes to pass this check.

4. Run the checks below and review CI. Do not release with unexplained failures.

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cd app && swift build && swift test
   cd ..
   ```

   CI uses `macos-latest` and the image's default Xcode. On 2026-11-16,
   [GitHub's image table](https://github.com/actions/runner-images#available-images)
   maps that label to macOS 26 arm64; its
   [software list](https://github.com/actions/runner-images/blob/main/images/macos/macos-26-arm64-Readme.md#xcode)
   lists Xcode 26.6 as default. Those versions move; CI logs the selected toolchain.

5. Select a full Developer ID Application identity and the stored profile:

   ```sh
   export SIGN_IDENTITY='Developer ID Application: YOUR CERTIFICATE NAME (TEAMID)'
   export NOTARY_PROFILE='<name>'
   scripts/release.sh 0.1.0 --dry-run
   ```

   A valid local signing identity is required. `NOTARY_PROFILE` is required only
   for a real submission; omit it for a dry run.
   The script checks versions, builds both architectures, uses the bundler's
   isolated verification output, replaces its nested CLI and Swift executable,
   then signs the CLI and app with hardened runtime and timestamps. Signing may
   contact Apple's timestamp service. It does not print credentials.

   A dry run writes signed archives and checksums to `dist/dry-run-VERSION/`,
   prints both zip paths it would upload, and attempts local assessments without
   printing the certificate name or team ID. It skips
   notarization and stapling; missing-ticket assessment failures are expected and
   reported. These archives are **not release-ready**. The profile's credentials
   are not validated until a real submission. Move previous outputs aside before
   retrying; existing final artifacts are never overwritten.

6. Review the dry-run artifacts, then run `scripts/release.sh 0.1.0` when ready.
   This uploads two signed zip submissions using the keychain profile and waits
   for **Accepted** results. It staples the app, checks signatures, assesses both
   executables with Gatekeeper, and validates the app's staple. A bare CLI cannot
   be stapled; its zip submission registers the signed binary for online Gatekeeper
   assessment. An offline first run of the CLI may therefore need network access.
   The app zip is recreated after stapling. A failed step stops the release.

7. Inspect `dist/Procwho-VERSION.zip`,
   `dist/procwho-VERSION-macos-universal.tar.gz`, and `dist/SHA256SUMS`. Staging
   directories and notarization receipts stay under `dist/.release.*` for inspection.
   Verify checksums with `cd dist && shasum -a 256 -c SHA256SUMS`. Test the downloaded
   artifacts on Apple Silicon and Intel, including Gatekeeper on another Mac.
   A local signature check alone does not prove the download experience works.

8. Prepare release notes outside the repository, using the changelog entry. The
   script prints a `gh release create` command with the exact source commit and
   three assets; replace `RELEASE-NOTES.md` with that notes file. Review the command
   before running it. The script never creates a tag or GitHub release itself.
   Confirm published asset checksums and download links after publication.

9. After publication, update `buddyh/homebrew-tap`. In `procwho.rb`, update `url`
   and `sha256` for the CLI tarball. In `Casks/procwho.rb`, update `version` and
   `sha256` for the app zip. Take both hashes from `dist/SHA256SUMS`. Then check and
   test both:

   ```sh
   brew audit --strict --online --formula buddyh/tap/procwho
   brew audit --strict --online --cask buddyh/tap/procwho
   brew style buddyh/tap
   brew install --formula buddyh/tap/procwho && procwho --version
   brew install --cask buddyh/tap/procwho
   brew uninstall --formula procwho
   brew uninstall --cask procwho
   ```

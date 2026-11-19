#!/bin/bash
# Local release packaging only. Never tags, publishes, installs, or launches.
set -euo pipefail
trap 'printf "release failed at line %s; no release was published\n" "$LINENO" >&2' ERR

fail() { printf 'release: %s\n' "$*" >&2; exit 1; }
usage() { echo 'Usage: scripts/release.sh VERSION [--dry-run]' >&2; exit 2; }

DRY_RUN=false
VERSION=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) $DRY_RUN && usage; DRY_RUN=true ;;
        -*) usage ;;
        *) [[ -z "$VERSION" ]] || usage; VERSION="$arg" ;;
    esac
done
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[[ "$(uname -s)" == Darwin ]] || fail 'macOS is required'
[[ -z "$(git status --porcelain --untracked-files=all)" ]] || fail 'working tree is dirty; commit or remove your own pending changes first'
SOURCE_HEAD="$(git rev-parse HEAD)"
check_source() {
    [[ -z "$(git status --porcelain --untracked-files=all)" && "$(git rev-parse HEAD)" == "$SOURCE_HEAD" ]] || fail 'source changed during packaging; retry from a stable clean tree'
}
[[ -n "${SIGN_IDENTITY:-}" ]] || fail 'set SIGN_IDENTITY to a Developer ID Application identity'
if ! $DRY_RUN; then
    [[ -n "${NOTARY_PROFILE:-}" ]] || fail 'set NOTARY_PROFILE to an existing notarytool keychain profile'
fi
[[ "$SIGN_IDENTITY" == 'Developer ID Application: '* ]] || fail 'SIGN_IDENTITY must name a Developer ID Application certificate'

for tool in cargo rustup swift lipo codesign security xcrun ditto tar shasum python3; do
    command -v "$tool" >/dev/null || fail "required tool is missing: $tool"
done
security find-identity -v -p codesigning | grep -F -- "\"$SIGN_IDENTITY\"" >/dev/null || fail 'signing identity is not available in the keychain'
[[ "$(tr -d '[:space:]' < app/VERSION)" == "$VERSION" ]] || fail 'app/VERSION does not match VERSION'
cargo metadata --locked --no-deps --format-version 1 | python3 -c '
import json, sys
version = sys.argv[1]
data = json.load(sys.stdin)
members = set(data["workspace_members"])
if any(p["version"] != version for p in data["packages"] if p["id"] in members):
    sys.exit("release: workspace package versions do not match VERSION")
' "$VERSION"

DIST="$ROOT/dist"
APP_ARCHIVE="Procwho-$VERSION.zip"
CLI_ARCHIVE="procwho-$VERSION-macos-universal.tar.gz"
$DRY_RUN && DIST="$DIST/dry-run-$VERSION"
for artifact in "$APP_ARCHIVE" "$CLI_ARCHIVE" SHA256SUMS; do
    [[ ! -e "$DIST/$artifact" ]] || fail "output already exists: $artifact (move it aside before retrying)"
done
mkdir -p "$DIST"
WORK="$(mktemp -d "$DIST/.release.XXXXXX")"
# Retain this run's staging files and submission receipts for inspection on failure.
echo "Staging in ${WORK#"$ROOT/"}"
export MACOSX_DEPLOYMENT_TARGET=13.0
export CARGO_TARGET_DIR="$WORK/target"
installed_targets="$(rustup target list --installed)"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    if ! grep -qx "$target" <<< "$installed_targets"; then
        echo "Adding Rust target: $target"
        rustup target add "$target"
    fi
    # Keep the builder's home and checkout paths out of the shipped binary.
    RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup --remap-path-prefix=$PWD=." \
        cargo build --locked --release -p procwho-cli --target "$target"
done
lipo -create "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/procwho" \
    "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/procwho" -output "$WORK/procwho"
# Verify each slice separately: some Apple lipo versions parse multiple names as inputs.
lipo "$WORK/procwho" -verify_arch arm64
lipo "$WORK/procwho" -verify_arch x86_64

# The bundler accepts neither a CLI path nor an identity. Use its isolated output,
# replace nested code, and re-sign. Build the Swift executable for both chips too.
app/scripts/bundle.sh --verify
APP="$WORK/Procwho.app"
ditto app/build/verification/Procwho.app "$APP"
export MACOSX_DEPLOYMENT_TARGET=14.0
swift build --package-path app --scratch-path "$WORK/swift" -c release --arch arm64 --arch x86_64
SWIFT_BIN="$(swift build --package-path app --scratch-path "$WORK/swift" -c release --arch arm64 --arch x86_64 --show-bin-path)"
cp "$SWIFT_BIN/ProcwhoBar" "$APP/Contents/MacOS/ProcwhoBar"
lipo "$APP/Contents/MacOS/ProcwhoBar" -verify_arch arm64
lipo "$APP/Contents/MacOS/ProcwhoBar" -verify_arch x86_64
cp "$WORK/procwho" "$APP/Contents/Resources/procwho"
check_source
codesign --force --sign "$SIGN_IDENTITY" --options runtime --timestamp "$WORK/procwho"
codesign --force --sign "$SIGN_IDENTITY" --options runtime --timestamp "$APP/Contents/Resources/procwho"
codesign --force --sign "$SIGN_IDENTITY" --options runtime --timestamp "$APP"
codesign --verify --strict "$WORK/procwho"
codesign --verify --strict "$APP/Contents/Resources/procwho"
codesign --verify --strict --deep "$APP"

# Names must differ beyond case: APFS is usually case-insensitive, so
# Procwho-X-notary.zip and procwho-X-notary.zip would be the same file.
APP_SUBMISSION="$WORK/app-$VERSION-notary.zip"
CLI_SUBMISSION="$WORK/cli-$VERSION-notary.zip"
ditto -c -k --keepParent "$APP" "$APP_SUBMISSION"
ditto -c -k --keepParent "$WORK/procwho" "$CLI_SUBMISSION"
check_source
if $DRY_RUN; then
    echo "DRY RUN: would upload ${APP_SUBMISSION#"$ROOT/"} and ${CLI_SUBMISSION#"$ROOT/"} with notarytool submit --wait."
    echo 'DRY RUN: skipping notarization and stapling; archives are not release-ready.'
else
    # --wait can return successfully with an Invalid status. Check JSON explicitly.
    submit() {
        local archive="$1" receipt="$2"
        xcrun notarytool submit "$archive" --keychain-profile "$NOTARY_PROFILE" --wait --output-format json > "$receipt"
        python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); sys.exit(0 if r.get("status") == "Accepted" else "release: notarization was not Accepted; inspect the submission receipt")' "$receipt"
    }
    submit "$APP_SUBMISSION" "$WORK/app-notary.json"
    submit "$CLI_SUBMISSION" "$WORK/cli-notary.json"
    # An Accepted ticket can take minutes to reach the staple service. Retry for ~10 minutes.
    for attempt in $(seq 1 20); do
        xcrun stapler staple "$APP" && break
        (( attempt < 20 )) || fail 'stapler could not fetch the notarization ticket; retry stapling the staged app later'
        echo "Waiting for the notarization ticket (attempt $attempt of 20)"
        sleep 30
    done
fi

# A bare CLI cannot be stapled. Its signed hash is registered by the zip submission;
# Gatekeeper needs network access to retrieve its notarization ticket.
codesign --verify --strict "$WORK/procwho"
codesign --verify --strict "$APP/Contents/Resources/procwho"
codesign --verify --strict --deep "$APP"
if $DRY_RUN; then
    # Assessments are still attempted, but missing tickets are expected in a dry run.
    spctl -a --type execute "$APP" >/dev/null 2>&1 || echo 'DRY RUN: app Gatekeeper assessment failed (no new notarization).'
    spctl -a --type execute "$WORK/procwho" >/dev/null 2>&1 || echo 'DRY RUN: CLI Gatekeeper assessment failed (no new notarization).'
    xcrun stapler validate "$APP" || echo 'DRY RUN: no valid stapled ticket was found.'
else
    spctl -a --type execute "$APP" >/dev/null 2>&1
    spctl -a --type execute "$WORK/procwho" >/dev/null 2>&1
    xcrun stapler validate "$APP"
fi
check_source
ditto -c -k --keepParent "$APP" "$DIST/$APP_ARCHIVE"
tar -czf "$DIST/$CLI_ARCHIVE" -C "$WORK" procwho
(cd "$DIST" && shasum -a 256 "$APP_ARCHIVE" "$CLI_ARCHIVE" > SHA256SUMS)
echo "Artifacts: ${DIST#"$ROOT/"} (staging and receipts retained)"
if $DRY_RUN; then
    echo 'DRY RUN: run again without --dry-run before publishing.'
fi
echo 'After a successful non-dry run, review and run:'
printf 'gh release create v%s --repo https://github.com/buddyh/procwho --target %s --title %s --notes-file RELEASE-NOTES.md dist/%s dist/%s dist/SHA256SUMS\n' \
    "$VERSION" "$SOURCE_HEAD" "\"$VERSION\"" "$APP_ARCHIVE" "$CLI_ARCHIVE"

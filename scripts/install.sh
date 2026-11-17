#!/bin/bash
# Install only on explicit request. No sudo, launch agent, or automatic app launch.
set -euo pipefail
trap 'printf "install failed at line %s\n" "$LINENO" >&2' ERR

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLI_DEST="$HOME/.local/bin/procwho"
APP_DEST="/Applications/Procwho.app"

case "${1:-cli}" in
    cli)
        [[ "$#" -le 1 ]] || exit 2
        [[ -x "$ROOT/target/release/procwho" ]] || {
            echo 'Build first: make build' >&2; exit 1;
        }
        mkdir -p "$(dirname "$CLI_DEST")"
        install -m 755 "$ROOT/target/release/procwho" "$CLI_DEST"
        echo 'Installed ~/.local/bin/procwho. Add ~/.local/bin to PATH if needed.'
        ;;
    app)
        [[ "$#" == 1 ]] || exit 2
        [[ -d "$ROOT/app/build/Procwho.app" ]] || {
            echo 'Build first: make app' >&2; exit 1;
        }
        [[ -w /Applications ]] || {
            echo '/Applications is not writable. Copy app/build/Procwho.app manually.' >&2; exit 1;
        }
        [[ ! -e "$APP_DEST" && ! -L "$APP_DEST" ]] || {
            echo '/Applications/Procwho.app already exists; remove it manually before replacing it.' >&2; exit 1;
        }
        ditto "$ROOT/app/build/Procwho.app" "$APP_DEST"
        echo 'Installed /Applications/Procwho.app.'
        ;;
    uninstall)
        [[ "$#" == 1 ]] || exit 2
        if [[ -f "$CLI_DEST" || -L "$CLI_DEST" ]]; then
            rm -f "$CLI_DEST"
        fi
        echo 'Removed ~/.local/bin/procwho. Remove /Applications/Procwho.app manually if installed.'
        echo 'Local history and configuration were retained.'
        ;;
    *) echo 'Usage: scripts/install.sh [cli|app|uninstall]' >&2; exit 2 ;;
esac

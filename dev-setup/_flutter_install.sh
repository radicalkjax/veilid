# Shared Flutter SDK installer for the veilidchat dev-setup (macOS + Linux). Sourced after
# versions.env (needs FLUTTER_VERSION). Flutter is a build dependency; flutter-packer can also
# provide it, in which case it's already on PATH and the dep_min version check skips this install.

# Managed, namespaced install dir so it never clobbers a dev's own flutter checkout.
FLUTTER_SDK_DIR="${FLUTTER_SDK_DIR:-$HOME/.local/share/veilid-flutter}"

# flutter --version -> "Flutter 3.44.0 • channel stable • ..."; empty when flutter isn't installed.
flutter_ver() { command -v flutter >/dev/null 2>&1 && flutter --version 2>/dev/null | head -1; }

# Put the flutter bin on this session's PATH + persist it to the user's shell rc (idempotent).
_flutter_persist_path() {
    local bin="$1" rc
    export PATH="$bin:$PATH"
    case "${SHELL:-}" in *zsh) rc="$HOME/.zshrc" ;; *bash) rc="$HOME/.bashrc" ;; *) rc="$HOME/.profile" ;; esac
    [ -e "$rc" ] || rc="$HOME/.profile"
    if ! grep -qsF "$bin" "$rc" 2>/dev/null; then
        printf '\n# Flutter SDK (veilidchat dev-setup)\nexport PATH="%s:$PATH"\n' "$bin" >> "$rc"
        echo "    added $bin to PATH ($rc)"
    fi
}

# Download + extract the pinned FLUTTER_VERSION archive for this OS/arch, then put it on PATH.
i_flutter() {
    local ver="$FLUTTER_VERSION" base="https://storage.googleapis.com/flutter_infra_release/releases/stable" url file tmp bin
    case "$(uname -s)" in
        Darwin) if [ "$(uname -m)" = arm64 ]; then url="$base/macos/flutter_macos_arm64_${ver}-stable.zip"; else url="$base/macos/flutter_macos_${ver}-stable.zip"; fi ;;
        Linux)  url="$base/linux/flutter_linux_${ver}-stable.tar.xz" ;;
        *) echo "    flutter: unsupported OS — install manually from https://docs.flutter.dev"; return 0 ;;
    esac
    echo "    installing Flutter $ver -> $FLUTTER_SDK_DIR/flutter"
    tmp="$(mktemp -d)"; file="$tmp/${url##*/}"
    if ! curl -fSL "$url" -o "$file"; then echo "    flutter download failed (network?)"; rm -rf "$tmp"; return 1; fi
    rm -rf "$FLUTTER_SDK_DIR"; mkdir -p "$FLUTTER_SDK_DIR"
    case "$file" in
        *.zip)    unzip -q "$file" -d "$FLUTTER_SDK_DIR" ;;
        *.tar.xz) tar -xJf "$file" -C "$FLUTTER_SDK_DIR" ;;
    esac
    rm -rf "$tmp"
    bin="$FLUTTER_SDK_DIR/flutter/bin"
    [ -x "$bin/flutter" ] || { echo "    flutter extract failed"; return 1; }
    _flutter_persist_path "$bin"
    flutter --version 2>/dev/null | head -1
}

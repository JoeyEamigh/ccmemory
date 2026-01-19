#!/bin/bash
# CCMemory binary downloader/updater
# Downloads the appropriate binary from GitHub releases
# Checks for updates and re-downloads if a newer version is available

set -e

BIN_DIR="$HOME/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ccmemory"
BINARY="$BIN_DIR/ccmemory"
VERSION_FILE="$DATA_DIR/.version"
REPO="JoeyEamigh/ccmemory"

# Detect platform
detect_platform() {
    local os arch
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)

    case "$os" in
        linux)  os="linux" ;;
        darwin) os="darwin" ;;
        mingw*|msys*|cygwin*) os="windows" ;;
        *) echo "Unsupported OS: $os" >&2; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64) arch="x64" ;;
        arm64|aarch64) arch="arm64" ;;
        *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

# Get latest release version from GitHub
get_latest_version() {
    local url="https://api.github.com/repos/$REPO/releases/latest"
    if command -v curl &>/dev/null; then
        curl -fsSL "$url" 2>/dev/null | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4
    elif command -v wget &>/dev/null; then
        wget -qO- "$url" 2>/dev/null | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4
    else
        echo ""
    fi
}

# Get currently installed version
get_installed_version() {
    if [ -f "$VERSION_FILE" ]; then
        cat "$VERSION_FILE"
    else
        echo ""
    fi
}

# Download binary from GitHub releases
download_binary() {
    local version="$1"
    local platform="$2"
    local filename="ccmemory-${platform}"

    # Windows executables have .exe extension
    if [[ "$platform" == windows-* ]]; then
        filename="${filename}.exe"
    fi

    local url="https://github.com/$REPO/releases/download/${version}/${filename}"

    echo "Downloading ccmemory ${version} for ${platform}..." >&2

    mkdir -p "$BIN_DIR"

    if command -v curl &>/dev/null; then
        curl -fsSL "$url" -o "$BINARY.tmp"
    elif command -v wget &>/dev/null; then
        wget -q "$url" -O "$BINARY.tmp"
    else
        echo "Error: curl or wget required" >&2
        exit 1
    fi

    mv "$BINARY.tmp" "$BINARY"
    chmod +x "$BINARY"
    mkdir -p "$DATA_DIR"
    echo "$version" > "$VERSION_FILE"

    echo "Downloaded ccmemory ${version}" >&2
}

# Main logic
main() {
    local platform latest_version installed_version

    platform=$(detect_platform)
    installed_version=$(get_installed_version)

    # Check if binary exists and is executable
    if [ -x "$BINARY" ] && [ -n "$installed_version" ]; then
        # Binary exists, check for updates (only if we can reach GitHub)
        latest_version=$(get_latest_version)

        if [ -n "$latest_version" ] && [ "$latest_version" != "$installed_version" ]; then
            echo "Update available: $installed_version -> $latest_version" >&2
            download_binary "$latest_version" "$platform"
        fi
    else
        # Binary doesn't exist, must download
        latest_version=$(get_latest_version)

        if [ -z "$latest_version" ]; then
            echo "Error: Cannot determine latest version. Check network connection." >&2
            exit 1
        fi

        download_binary "$latest_version" "$platform"
    fi

    # Output the binary path for use by callers
    echo "$BINARY"
}

main "$@"

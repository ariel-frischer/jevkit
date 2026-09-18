#!/bin/sh
# shellcheck disable=SC3043  # 'local' is widely supported in practice (dash, ash, busybox)
# jev (jevkit) Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/ariel-frischer/jevkit/main/install.sh | sh
#
# Downloads a prebuilt release binary. No Rust toolchain required.
#
# Environment variables:
#   JEV_INSTALL_DIR - Installation directory (default: ~/.local/bin)
#   JEV_VERSION     - Specific version to install (default: latest)

set -eu

# Configuration
GITHUB_REPO="ariel-frischer/jevkit"
BINARY_NAME="jev"
DEFAULT_INSTALL_DIR="$HOME/.local/bin"

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    RED='\e[0;31m'
    GREEN='\e[0;32m'
    YELLOW='\e[0;33m'
    BLUE='\e[0;34m'
    NC='\e[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

info() {
    printf '%b==>%b %s\n' "${BLUE}" "${NC}" "$1" >&2
}

success() {
    printf '%b==>%b %s\n' "${GREEN}" "${NC}" "$1" >&2
}

warn() {
    printf '%bWarning:%b %s\n' "${YELLOW}" "${NC}" "$1" >&2
}

error() {
    printf '%bError:%b %b\n' "${RED}" "${NC}" "$1" >&2
    exit 1
}

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*)
            printf '%bWindows Detected%b\n\n' "${YELLOW}" "${NC}" >&2
            printf 'jev requires WSL (Windows Subsystem for Linux).\n\n' >&2
            printf 'Install WSL and try again:\n\n' >&2
            printf '  1. Open PowerShell as Administrator and run:\n' >&2
            printf '     %bwsl --install%b\n\n' "${GREEN}" "${NC}" >&2
            printf '  2. Restart your computer\n\n' >&2
            printf '  3. Open the WSL terminal and re-run this installer\n\n' >&2
            exit 1
            ;;
        *) error "Unsupported operating system: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) error "Unsupported architecture: $(uname -m)" ;;
    esac
}

check_dependencies() {
    for cmd in curl tar; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            error "Required command not found: $cmd"
        fi
    done
}

get_latest_version() {
    # /releases/latest returns the newest non-prerelease, non-draft release.
    local latest_url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    local version

    version=$(curl -fsSL "$latest_url" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

    if [ -z "$version" ]; then
        error "Failed to fetch latest version from GitHub. Check your internet connection."
    fi

    echo "$version"
}

# Archive names match .github/workflows/release.yml exactly.
# Note: the release workflow builds gnu targets, not musl, so glibc is required.
download_and_verify() {
    local version="$1"
    local os="$2"
    local arch="$3"
    local tmp_dir="$4"

    local archive_name="jev-${arch}-${os}.tar.gz"
    local download_url="https://github.com/${GITHUB_REPO}/releases/download/${version}/${archive_name}"
    local checksum_url="${download_url}.sha256"

    info "Downloading ${archive_name}..."

    # Use progress bar (-#) if stderr is a terminal, silent (-s) otherwise
    local curl_opts="-fSL"
    if [ -t 2 ]; then
        curl_opts="-f#L"
    fi

    if ! curl $curl_opts -o "${tmp_dir}/${archive_name}" "$download_url"; then
        error "Failed to download ${archive_name}. Check if version ${version} exists."
    fi

    if curl -fsSL -o "${tmp_dir}/checksum.sha256" "$checksum_url" 2>/dev/null; then
        info "Verifying checksum..."
        local expected_checksum
        expected_checksum=$(grep "${archive_name}" "${tmp_dir}/checksum.sha256" | awk '{print $1}')

        if [ -n "$expected_checksum" ]; then
            local actual_checksum
            if command -v sha256sum >/dev/null 2>&1; then
                actual_checksum=$(sha256sum "${tmp_dir}/${archive_name}" | awk '{print $1}')
            elif command -v shasum >/dev/null 2>&1; then
                actual_checksum=$(shasum -a 256 "${tmp_dir}/${archive_name}" | awk '{print $1}')
            else
                warn "sha256sum/shasum not found, skipping checksum verification"
                echo "${tmp_dir}/${archive_name}"
                return
            fi

            if [ "$expected_checksum" != "$actual_checksum" ]; then
                error "Checksum verification failed!\nExpected: ${expected_checksum}\nActual: ${actual_checksum}"
            fi
            success "Checksum verified"
        else
            warn "Checksum not found for ${archive_name}, skipping verification"
        fi
    else
        warn "Could not download checksum, skipping verification"
    fi

    echo "${tmp_dir}/${archive_name}"
}

extract_binary() {
    local archive_path="$1"
    local tmp_dir="$2"

    info "Extracting archive..."
    tar -xzf "$archive_path" -C "$tmp_dir"

    if [ ! -f "${tmp_dir}/${BINARY_NAME}" ]; then
        error "Binary '${BINARY_NAME}' not found in archive"
    fi

    echo "${tmp_dir}/${BINARY_NAME}"
}

install_binary() {
    local binary_path="$1"
    local install_dir="$2"

    if [ ! -d "$install_dir" ]; then
        info "Creating directory ${install_dir}..."
        if ! mkdir -p "$install_dir" 2>/dev/null; then
            warn "Cannot create ${install_dir} without elevated privileges"
            info "Trying with sudo..."
            sudo mkdir -p "$install_dir"
        fi
    fi

    if [ -w "$install_dir" ]; then
        mv "$binary_path" "${install_dir}/${BINARY_NAME}"
        chmod +x "${install_dir}/${BINARY_NAME}"
    else
        info "Elevated privileges required to install to ${install_dir}"
        sudo mv "$binary_path" "${install_dir}/${BINARY_NAME}"
        sudo chmod +x "${install_dir}/${BINARY_NAME}"
    fi
}

version_gte() {
    local v1="$1"
    local v2="$2"
    v1="${v1#v}"
    v2="${v2#v}"
    if printf '%s\n%s' "$v2" "$v1" | sort -V -C 2>/dev/null; then
        return 0
    fi
    [ "$v1" = "$v2" ] || [ "$(printf '%s\n%s' "$v1" "$v2" | sort -V | tail -1)" = "$v1" ]
}

check_path() {
    case ":$PATH:" in
        *":$1:"*) return 0 ;;
        *) return 1 ;;
    esac
}

main() {
    echo ""
    printf '%b%s\n' "${GREEN}" '     ██╗███████╗██╗   ██╗'
    printf '%s%b\n' '     ██║██╔════╝██║   ██║' "${NC}"
    printf '%s%b\n' '     ██║█████╗  ██║   ██║' "${NC}"
    printf '%s%b\n' '██   ██║██╔══╝  ╚██╗ ██╔╝' "${NC}"
    printf '%s%b\n' '╚█████╔╝███████╗ ╚████╔╝ ' "${NC}"
    printf '%s%b\n' ' ╚════╝ ╚══════╝  ╚═══╝  ' "${NC}"
    echo "         Installer"
    echo ""

    check_dependencies

    local os arch
    os=$(detect_os)
    arch=$(detect_arch)
    info "Detected platform: ${os}/${arch}"

    local version="${JEV_VERSION:-}"
    if [ -z "$version" ]; then
        info "Fetching latest version..."
        version=$(get_latest_version)
    fi
    info "Installing version: ${version}"

    local install_dir="${JEV_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
    info "Install directory: ${install_dir}"

    local target_binary="${install_dir}/${BINARY_NAME}"

    if [ -f "$target_binary" ]; then
        local existing_version
        existing_version=$("$target_binary" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "")
        if [ -n "$existing_version" ]; then
            info "Found existing installation: ${existing_version}"
            if version_gte "v${existing_version}" "$version"; then
                success "Already up-to-date (installed: ${existing_version}, requested: ${version})"
                echo ""
                echo "To force reinstall, remove the existing binary first:"
                echo "    rm ${target_binary}"
                echo ""
                exit 0
            fi
            info "Upgrading from ${existing_version} to ${version}"
        else
            info "Found existing installation (unknown version)"
        fi
    fi

    local tmp_dir
    tmp_dir=$(mktemp -d)
    cleanup() { rm -rf "$tmp_dir"; }
    trap cleanup EXIT
    trap 'warn "Interrupted, cleaning up..."; cleanup; exit 1' INT TERM

    local archive_path binary_path
    archive_path=$(download_and_verify "$version" "$os" "$arch" "$tmp_dir")
    binary_path=$(extract_binary "$archive_path" "$tmp_dir")

    info "Installing to ${install_dir}..."
    install_binary "$binary_path" "$install_dir"

    info "Verifying installation..."
    if ! "$target_binary" --version >/dev/null 2>&1; then
        error "Installed binary failed verification. Remove it and try again:  rm ${target_binary}"
    fi
    success "Verification passed"

    echo ""
    success "Successfully installed ${BINARY_NAME} ${version} to ${install_dir}/${BINARY_NAME}"
    echo ""

    if check_path "$install_dir"; then
        success "${install_dir} is already in your PATH"
    else
        warn "${install_dir} is NOT in your PATH"
        echo ""
        echo "Add it to your shell config:"
        echo ""
        echo "    # Bash (~/.bashrc) or Zsh (~/.zshrc)"
        echo "    export PATH=\"${install_dir}:\$PATH\""
        echo ""
        echo "    # Fish (~/.config/fish/config.fish)"
        echo "    fish_add_path ${install_dir}"
        echo ""
        echo "Then reload your shell."
        echo ""
    fi

    echo "Get started:"
    echo ""
    echo "    jev --version"
    echo "    jev init"
    echo "    jev --help"
    echo ""
    echo "Documentation: https://github.com/${GITHUB_REPO}"
    echo ""

    rm -rf "$tmp_dir"
    trap - EXIT INT TERM
}

main "$@"

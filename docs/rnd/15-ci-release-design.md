# 15 — CI/CD & Release Infrastructure Design

> **Status:** Design
> **Owner:** CI/CD & Release Engineer
> **Date:** 2025-06

---

## Table of Contents

1. [Current State](#1-current-state)
2. [CI Pipeline — Full Design](#2-ci-pipeline--full-design)
3. [Release Pipeline — Full Design](#3-release-pipeline--full-design)
4. [Install Script (`install.sh`)](#4-install-script-installsh)
5. [Homebrew Tap](#5-homebrew-tap)
6. [cargo install](#6-cargo-install)
7. [Documentation Site](#7-documentation-site)
8. [npm Wrapper Package](#8-npm-wrapper-package)
9. [Implementation Roadmap](#9-implementation-roadmap)

---

## 1. Current State

The project already has minimal CI and release workflows:

- **CI** (`ci.yml`): Tests on ubuntu/macos/windows with clippy and fmt. Missing: coverage, binary size checks, release build verification.
- **Release** (`release.yml`): Tag-triggered builds with a 5-target matrix. Missing: cross-compilation tooling for aarch64-linux, SHA256 checksums, changelog generation, musl static builds.

This document replaces both files with production-grade equivalents and adds install scripts, Homebrew, npm, and docs site infrastructure.

---

## 2. CI Pipeline — Full Design

### 2.1 Design Goals

| Requirement | Implementation |
|---|---|
| Multi-OS testing | ubuntu-latest, macos-latest, windows-latest |
| Lint enforcement | `cargo clippy -- -D warnings` |
| Format enforcement | `cargo fmt --all -- --check` |
| Test coverage | `cargo-llvm-cov` (faster than tarpaulin, works on all 3 OSes) |
| Binary size guard | Fail if release binary > 4 MB |
| Release smoke test | Build release binary, run `snip --version` and `snip --help` |
| Caching | `Swatinem/rust-cache@v2` per target |

### 2.2 Why `cargo-llvm-cov` over `tarpaulin`

- **tarpaulin** only works on Linux (requires ptrace), slow.
- **cargo-llvm-cov** works on Linux, macOS, and Windows. Uses LLVM's source-based coverage. 5-10x faster.
- We run coverage on ubuntu-latest only and upload the HTML report as an artifact.

### 2.3 Complete CI Workflow YAML

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  # ── Lint & Format (single job, fast feedback) ──────────────────
  lint:
    name: Lint & Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Check formatting
        run: cargo fmt --all -- --check
      - name: Clippy (all targets, all features)
        run: cargo clippy --all-targets --all-features -- -D warnings

  # ── Test on all 3 OSes ────────────────────────────────────────
  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    needs: lint
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.os }}
      - name: Run tests
        run: cargo test --all-features

  # ── Coverage (ubuntu only, cargo-llvm-cov) ────────────────────
  coverage:
    name: Coverage
    runs-on: ubuntu-latest
    needs: lint
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview
      - uses: Swatinem/rust-cache@v2
        with:
          key: coverage
      - name: Install cargo-llvm-cov
        run: cargo install cargo-llvm-cov --locked
      - name: Generate coverage report
        run: cargo llvm-cov --all-features --html --output-dir coverage/
      - name: Print coverage summary
        run: cargo llvm-cov --all-features
      - name: Upload coverage HTML
        uses: actions/upload-artifact@v4
        with:
          name: coverage-report
          path: coverage/
          retention-days: 30

  # ── Binary size check (release build, < 4 MB) ────────────────
  binary-size:
    name: Binary Size Check
    runs-on: ubuntu-latest
    needs: lint
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: binary-size
      - name: Build release
        run: cargo build --release
      - name: Check binary size (max 4 MB)
        shell: bash
        run: |
          BINARY="target/release/snip"
          SIZE=$(stat -c%s "$BINARY" 2>/dev/null || stat -f%z "$BINARY" 2>/dev/null)
          MAX_SIZE=4194304  # 4 MB in bytes
          echo "Binary size: $(( SIZE / 1024 )) KB ($SIZE bytes)"
          if [ "$SIZE" -gt "$MAX_SIZE" ]; then
            echo "::error::Binary size $SIZE bytes exceeds 4 MB limit"
            exit 1
          fi

  # ── Release build smoke test ──────────────────────────────────
  release-smoke:
    name: Release Smoke Test
    runs-on: ubuntu-latest
    needs: lint
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          key: release-smoke
      - name: Build release
        run: cargo build --release
      - name: Verify snip --version
        run: ./target/release/snip --version
      - name: Verify snip --help
        run: ./target/release/snip --help
      - name: Verify snip doctor
        run: ./target/release/snip doctor
```

### 2.4 CI Flow Diagram

```
push/PR ──► lint ──► test (ubuntu, macos, windows)
              │         ├──► coverage (ubuntu)
              │         ├──► binary-size (ubuntu)
              │         └──► release-smoke (ubuntu)
              └──► (parallel fan-out after lint passes)
```

### 2.5 Key Decisions

| Decision | Rationale |
|---|---|
| `lint` as a gate job | Fast feedback (< 30s). test/coverage/size all wait for it. |
| `concurrency` with cancel-in-progress | Save CI minutes on rapid pushes. |
| `fail-fast: false` on test matrix | Don't cancel macos/windows if ubuntu fails. See all failures. |
| 4 MB binary size limit | Keeps snip lean. Current deps (clap, serde, etc.) should stay well under. |
| `cargo-llvm-cov` | Works on all platforms, fast, accurate source-based coverage. |

---

## 3. Release Pipeline — Full Design

### 3.1 Build Matrix

| Target | OS Runner | Toolchain | Binary Name | Asset Name |
|---|---|---|---|---|
| `x86_64-unknown-linux-musl` | ubuntu-latest | `cargo-zigbuild` + zig | `snip` | `snip-x86_64-linux-musl.tar.gz` |
| `aarch64-unknown-linux-musl` | ubuntu-latest | `cargo-zigbuild` + zig | `snip` | `snip-aarch64-linux-musl.tar.gz` |
| `x86_64-apple-darwin` | macos-latest | stable | `snip` | `snip-x86_64-macos.tar.gz` |
| `aarch64-apple-darwin` | macos-latest | stable | `snip` | `snip-aarch64-macos.tar.gz` |
| `x86_64-pc-windows-msvc` | windows-latest | stable | `snip.exe` | `snip-x86_64-windows.zip` |

### 3.2 Cross-Compilation Strategy

**Linux (aarch64):** Use `cargo-zigbuild` with Zig as the C cross-compiler. This avoids the complexity of setting up `cross-rs` Docker containers and produces true musl-static binaries.

**Why `cargo-zigbuild` over `cross-rs`:**
- No Docker required (faster CI, no Docker-in-Docker issues on GitHub Actions)
- Produces static musl binaries without extra config
- Single action install: `github.com/mliezun/setup-zig@v1`
- Works for both x86_64 and aarch64 musl targets

**macOS (x86_64 on M1 runner):** Rust's stable toolchain handles universal / cross-compilation natively via `--target x86_64-apple-darwin` on Apple Silicon runners.

### 3.3 Complete Release Workflow YAML

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'

env:
  CARGO_TERM_COLOR: always

permissions:
  contents: write

jobs:
  # ── Build all release targets ─────────────────────────────────
  build-release:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          # Linux — static musl via cargo-zigbuild
          - os: ubuntu-latest
            target: x86_64-unknown-linux-musl
            artifact_name: snip
            asset_name: snip-x86_64-linux-musl
            archive_ext: tar.gz
          - os: ubuntu-latest
            target: aarch64-unknown-linux-musl
            artifact_name: snip
            asset_name: snip-aarch64-linux-musl
            archive_ext: tar.gz
          # macOS
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact_name: snip
            asset_name: snip-x86_64-macos
            archive_ext: tar.gz
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact_name: snip
            asset_name: snip-aarch64-macos
            archive_ext: tar.gz
          # Windows
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact_name: snip.exe
            asset_name: snip-x86_64-windows
            archive_ext: zip
    steps:
      - uses: actions/checkout@v4

      # ── Toolchain setup ──
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      # ── Zig for Linux cross-compilation ──
      - name: Install Zig (Linux cross-compilation)
        if: contains(matrix.target, 'linux-musl')
        uses: mliezun/setup-zig@v1
        with:
          version: 0.13.0

      - name: Install cargo-zigbuild (Linux cross-compilation)
        if: contains(matrix.target, 'linux-musl')
        run: cargo install cargo-zigbuild --locked

      # ── Cache ──
      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      # ── Build ──
      - name: Build (Linux musl via zigbuild)
        if: contains(matrix.target, 'linux-musl')
        run: cargo zigbuild --release --target ${{ matrix.target }}

      - name: Build (native)
        if: ${{ !contains(matrix.target, 'linux-musl') }}
        run: cargo build --release --target ${{ matrix.target }}

      # ── Package ──
      - name: Package (tar.gz)
        if: matrix.archive_ext == 'tar.gz'
        run: |
          cd target/${{ matrix.target }}/release
          tar czf ../../../${{ matrix.asset_name }}.tar.gz ${{ matrix.artifact_name }}

      - name: Package (zip)
        if: matrix.archive_ext == 'zip'
        shell: pwsh
        run: |
          Compress-Archive -Path "target/${{ matrix.target }}/release/${{ matrix.artifact_name }}" -DestinationPath "${{ matrix.asset_name }}.zip"

      # ── Upload artifact for release job ──
      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.asset_name }}
          path: ${{ matrix.asset_name }}.${{ matrix.archive_ext }}

  # ── Generate checksums, changelog, and publish ────────────────
  release:
    name: Publish Release
    needs: build-release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # need full history for changelog

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      # ── SHA256 checksums ──
      - name: Generate SHA256 checksums
        working-directory: artifacts
        run: sha256sum * > ../checksums-sha256.txt

      - name: Upload checksums artifact
        uses: actions/upload-artifact@v4
        with:
          name: checksums-sha256
          path: checksums-sha256.txt

      # ── Changelog from git log ──
      - name: Generate changelog
        id: changelog
        run: |
          # Get the previous tag (if any)
          PREV_TAG=$(git tag --sort=-version:refname | sed -n '2p')
          VERSION="${GITHUB_REF_NAME}"

          if [ -z "$PREV_TAG" ]; then
            echo "No previous tag found — generating changelog from first commit"
            LOG=$(git log --pretty=format:"- %s (%h)" HEAD)
          else
            echo "Generating changelog from ${PREV_TAG} to ${VERSION}"
            LOG=$(git log --pretty=format:"- %s (%h)" "${PREV_TAG}..HEAD")
          fi

          # Write to file for the release body
          {
            echo "## What's Changed in ${VERSION}"
            echo ""
            echo "$LOG"
            echo ""
            echo "---"
            echo ""
            echo "**Full Changelog**: https://github.com/Bilal140202/snip/compare/${PREV_TAG:-${VERSION}\~1}...${VERSION}"
          } > CHANGELOG.md

          cat CHANGELOG.md

      # ── Create GitHub Release ──
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            artifacts/*
            checksums-sha256.txt
          body_path: CHANGELOG.md
          draft: false
          prerelease: ${{ contains(github.ref_name, '-rc') || contains(github.ref_name, '-beta') || contains(github.ref_name, '-alpha') }}
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### 3.4 Release Artifacts

Each release will produce:

```
snip-x86_64-linux-musl.tar.gz
snip-aarch64-linux-musl.tar.gz
snip-x86_64-macos.tar.gz
snip-aarch64-macos.tar.gz
snip-x86_64-windows.zip
checksums-sha256.txt
```

### 3.5 Pre-release Detection

Tags containing `-rc`, `-beta`, or `-alpha` are automatically marked as pre-releases on GitHub.

### 3.6 Release Flow Diagram

```
git tag v0.1.0 && git push --tags
    │
    ├──► Build (x86_64-linux-musl)      ──┐
    ├──► Build (aarch64-linux-musl)     ──┤
    ├──► Build (x86_64-macos)           ──┤
    ├──► Build (aarch64-macos)          ──┤
    └──► Build (x86_64-windows)         ──┘
                                           │
                                    all succeed?
                                           │
                                     ┌─────▼─────┐
                                     │  Release   │
                                     │  - SHA256  │
                                     │  - Changelog│
                                     │  - Upload  │
                                     └───────────┘
```

---

## 4. Install Script (`install.sh`)

### 4.1 Design

The install script follows the pattern established by `ripgrep`, `fd`, `bat`, `starship`, and other popular Rust CLI tools.

### 4.2 Complete `install.sh`

```bash
#!/usr/bin/env bash
#
# install.sh — Install snip from GitHub Releases
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Bilal140202/snip/main/install.sh | bash
#
# Environment variables:
#   SNIP_INSTALL_DIR  — Override install directory (default: ~/.local/bin)
#   SNIP_VERSION      — Install a specific version (default: latest)
#   GITHUB_TOKEN      — For private repos (not needed for public)
#

set -euo pipefail

REPO="Bilal140202/snip"
BINARY_NAME="snip"
DEFAULT_INSTALL_DIR="$HOME/.local/bin"

# ── Helpers ──────────────────────────────────────────────────────

info()  { printf "\033[1;34m[info]\033[0m  %s\n" "$1"; }
error() { printf "\033[1;31m[error]\033[0m %s\n" "$1" >&2; exit 1; }

# ── Detect OS ───────────────────────────────────────────────────

detect_os() {
    local os
    os="$(uname -s)"
    case "$os" in
        Linux*)     echo "linux"   ;;
        Darwin*)    echo "macos"   ;;
        MINGW*|MSYS*|CYGWIN*)
                    echo "windows" ;;
        *)          error "Unsupported OS: $os" ;;
    esac
}

# ── Detect Architecture ─────────────────────────────────────────

detect_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64|amd64)  echo "x86_64"  ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $arch" ;;
    esac
}

# ── Get latest release tag ──────────────────────────────────────

get_latest_version() {
    if [ -n "${SNIP_VERSION:-}" ]; then
        echo "$SNIP_VERSION"
        return
    fi
    local url="https://api.github.com/repos/${REPO}/releases/latest"
    local version
    version=$(curl -fsSL "$url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    [ -z "$version" ] && error "Could not determine latest version. Set SNIP_VERSION=<version> to override."
    echo "$version"
}

# ── Main ────────────────────────────────────────────────────────

main() {
    local os arch version install_dir

    os="$(detect_os)"
    arch="$(detect_arch)"
    version="$(get_latest_version)"
    install_dir="${SNIP_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

    info "Detected: ${os}/${arch}"
    info "Version:  ${version}"
    info "Install:  ${install_dir}"

    # ── Determine asset name ──
    local asset_name archive_ext
    if [ "$os" = "windows" ]; then
        asset_name="${BINARY_NAME}-${arch}-windows"
        archive_ext="zip"
    elif [ "$os" = "macos" ]; then
        asset_name="${BINARY_NAME}-${arch}-macos"
        archive_ext="tar.gz"
    else
        asset_name="${BINARY_NAME}-${arch}-linux-musl"
        archive_ext="tar.gz"
    fi

    local download_url="https://github.com/${REPO}/releases/download/${version}/${asset_name}.${archive_ext}"
    local checksum_url="https://github.com/${REPO}/releases/download/${version}/checksums-sha256.txt"

    # ── Create temp dir ──
    local tmp_dir
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "$tmp_dir"' EXIT

    # ── Download binary ──
    info "Downloading ${asset_name}.${archive_ext}..."
    curl -fsSL "$download_url" -o "${tmp_dir}/${asset_name}.${archive_ext}"

    # ── Download checksums and verify ──
    info "Verifying SHA256 checksum..."
    curl -fsSL "$checksum_url" -o "${tmp_dir}/checksums-sha256.txt"

    local expected_checksum
    expected_checksum=$(grep "${asset_name}.${archive_ext}" "${tmp_dir}/checksums-sha256.txt" | awk '{print $1}')
    [ -z "$expected_checksum" ] && error "Checksum not found for ${asset_name}.${archive_ext}"

    local actual_checksum
    if command -v sha256sum &>/dev/null; then
        actual_checksum=$(sha256sum "${tmp_dir}/${asset_name}.${archive_ext}" | awk '{print $1}')
    elif command -v shasum &>/dev/null; then
        actual_checksum=$(shasum -a 256 "${tmp_dir}/${asset_name}.${archive_ext}" | awk '{print $1}')
    else
        error "Neither sha256sum nor shasum found — cannot verify checksum"
    fi

    if [ "$expected_checksum" != "$actual_checksum" ]; then
        error "Checksum mismatch!\n  expected: ${expected_checksum}\n  actual:   ${actual_checksum}"
    fi
    info "Checksum verified OK"

    # ── Extract ──
    info "Extracting..."
    cd "$tmp_dir"
    if [ "$archive_ext" = "tar.gz" ]; then
        tar xzf "${asset_name}.${archive_ext}"
    else
        unzip -qo "${asset_name}.${archive_ext}"
    fi

    # ── Install ──
    mkdir -p "$install_dir"
    cp "${tmp_dir}/${BINARY_NAME}" "${install_dir}/${BINARY_NAME}"
    chmod +x "${install_dir}/${BINARY_NAME}"

    info "Installed ${BINARY_NAME} to ${install_dir}/${BINARY_NAME}"

    # ── PATH check ──
    case ":$PATH:" in
        *":${install_dir}:"*) ;;
        *)
            info ""
            info "${install_dir} is not in your PATH."
            info "Add it by running:"
            info ""
            info "    echo 'export PATH=\"${install_dir}:\$PATH\"' >> ~/.bashrc"
            info "    source ~/.bashrc"
            info ""
            ;;
    esac

    # ── Verify ──
    info "Verifying installation..."
    "${install_dir}/${BINARY_NAME}" --version || true

    info ""
    info "Done! Run 'snip --help' to get started."
}

main
```

### 4.3 Install Script Features

| Feature | Detail |
|---|---|
| OS detection | Linux, macOS, Windows (MSYS2/Git Bash) |
| Arch detection | x86_64, aarch64 |
| Version selection | `SNIP_VERSION=v0.1.0` env var, defaults to latest |
| Install directory | `SNIP_INSTALL_DIR` override, defaults to `~/.local/bin` |
| Checksum verification | Downloads `checksums-sha256.txt`, verifies before install |
| PATH detection | Warns if install dir not in PATH, prints the `export` command |
| Cleanup | Uses `mktemp -d` with `trap ... EXIT` for automatic cleanup |

---

## 5. Homebrew Tap

### 5.1 Strategy

Create a minimal tap repository that users add with:

```bash
brew tap Bilal140202/tap
brew install snip
```

### 5.2 Tap Repository Structure

```
Bilal140202/homebrew-tap/
├── Formula/
│   └── snip.rb
├── README.md
└── .github/
    └── workflows/
        └── update-formula.yml    # auto-update on new release
```

### 5.3 Homebrew Formula (`Formula/snip.rb`)

```ruby
class Snip < Formula
  desc "Project-scoped command snippets with built-in fuzzy finder"
  homepage "https://github.com/Bilal140202/snip"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Bilal140202/snip/releases/download/v0.1.0/snip-aarch64-macos.tar.gz"
      sha256 "PLACEHOLDER_AARCH64_MACOS"
    end
    on_intel do
      url "https://github.com/Bilal140202/snip/releases/download/v0.1.0/snip-x86_64-macos.tar.gz"
      sha256 "PLACEHOLDER_X86_64_MACOS"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Bilal140202/snip/releases/download/v0.1.0/snip-aarch64-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_AARCH64_LINUX"
    end
    on_intel do
      url "https://github.com/Bilal140202/snip/releases/download/v0.1.0/snip-x86_64-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_X86_64_LINUX"
    end
  end

  def install
    bin.install "snip"
  end

  test do
    assert_match "snip", shell_output("#{bin}/snip --version")
    assert_match "snip", shell_output("#{bin}/snip --help")
  end
end
```

### 5.4 Auto-Update Workflow (in tap repo)

```yaml
# .github/workflows/update-formula.yml (in Bilal140202/homebrew-tap)
name: Update Formula

on:
  repository_dispatch:
    types: [update-formula]
  workflow_dispatch:
    inputs:
      version:
        description: 'Version (e.g. 0.2.0)'
        required: true

jobs:
  update:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4

      - name: Determine version
        id: version
        run: |
          if [ "${{ github.event_name }}" = "repository_dispatch" ]; then
            echo "version=${{ github.event.client_payload.version }}" >> "$GITHUB_OUTPUT"
          else
            echo "version=${{ github.event.inputs.version }}" >> "$GITHUB_OUTPUT"
          fi

      - name: Download checksums
        run: |
          curl -fsSL "https://github.com/Bilal140202/snip/releases/download/v${{ steps.version.outputs.version }}/checksums-sha256.txt" \
            -o checksums-sha256.txt

      - name: Update formula
        run: |
          VERSION="${{ steps.version.outputs.version }}"

          # Extract SHA256 hashes
          SHA_AARCH64_MACOS=$(grep "snip-aarch64-macos.tar.gz" checksums-sha256.txt | awk '{print $1}')
          SHA_X86_64_MACOS=$(grep "snip-x86_64-macos.tar.gz" checksums-sha256.txt | awk '{print $1}')
          SHA_AARCH64_LINUX=$(grep "snip-aarch64-linux-musl.tar.gz" checksums-sha256.txt | awk '{print $1}')
          SHA_X86_64_LINUX=$(grep "snip-x86_64-linux-musl.tar.gz" checksums-sha256.txt | awk '{print $1}')

          # Generate formula
          cat > Formula/snip.rb << 'RUBY'
          class Snip < Formula
            desc "Project-scoped command snippets with built-in fuzzy finder"
            homepage "https://github.com/Bilal140202/snip"
            version "${VERSION}"
            license "MIT"

            on_macos do
              on_arm do
                url "https://github.com/Bilal140202/snip/releases/download/v${VERSION}/snip-aarch64-macos.tar.gz"
                sha256 "${SHA_AARCH64_MACOS}"
              end
              on_intel do
                url "https://github.com/Bilal140202/snip/releases/download/v${VERSION}/snip-x86_64-macos.tar.gz"
                sha256 "${SHA_X86_64_MACOS}"
              end
            end

            on_linux do
              on_arm do
                url "https://github.com/Bilal140202/snip/releases/download/v${VERSION}/snip-aarch64-linux-musl.tar.gz"
                sha256 "${SHA_AARCH64_LINUX}"
              end
              on_intel do
                url "https://github.com/Bilal140202/snip/releases/download/v${VERSION}/snip-x86_64-linux-musl.tar.gz"
                sha256 "${SHA_X86_64_LINUX}"
              end
            end

            def install
              bin.install "snip"
            end

            test do
              assert_match "snip", shell_output("\#{bin}/snip --version")
              assert_match "snip", shell_output("\#{bin}/snip --help")
            end
          end
          RUBY

          # Fix heredoc variable interpolation — use sed
          sed -i "s/\${VERSION}/${VERSION}/g" Formula/snip.rb
          sed -i "s/\${SHA_AARCH64_MACOS}/${SHA_AARCH64_MACOS}/g" Formula/snip.rb
          sed -i "s/\${SHA_X86_64_MACOS}/${SHA_X86_64_MACOS}/g" Formula/snip.rb
          sed -i "s/\${SHA_AARCH64_LINUX}/${SHA_AARCH64_LINUX}/g" Formula/snip.rb
          sed -i "s/\${SHA_X86_64_LINUX}/${SHA_X86_64_LINUX}/g" Formula/snip.rb

      - name: Commit and push
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add Formula/snip.rb
          git commit -m "Update snip to v${{ steps.version.outputs.version }}" || echo "No changes"
          git push
```

### 5.5 Trigger from Main Release Pipeline

Add this step to the release job in `release.yml`:

```yaml
      # ── Trigger Homebrew tap update ──
      - name: Trigger Homebrew formula update
        if: ${{ !contains(github.ref_name, '-rc') && !contains(github.ref_name, '-beta') && !contains(github.ref_name, '-alpha') }}
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          curl -X POST \
            -H "Accept: application/vnd.github+json" \
            -H "Authorization: Bearer ${{ secrets.TAP_REPO_PAT }}" \
            "https://api.github.com/repos/Bilal140202/homebrew-tap/dispatches" \
            -d "{\"event_type\":\"update-formula\",\"client_payload\":{\"version\":\"$VERSION\"}}"
```

> **Note:** Requires a `TAP_REPO_PAT` secret with write access to `Bilal140202/homebrew-tap`. A fine-grained PAT with `contents: write` on the tap repo is sufficient.

---

## 6. `cargo install`

### 6.1 Crates.io Publishing

The `Cargo.toml` already has the required metadata fields. To publish:

```bash
# One-time: login
cargo login <API_TOKEN>

# Dry run to verify
cargo publish --dry-run

# Publish
cargo publish
```

### 6.2 Required `Cargo.toml` Additions

```toml
[package]
name = "snip"
version = "0.1.0"
edition = "2021"
description = "Project-scoped command snippets with built-in fuzzy finder"
license = "MIT"
repository = "https://github.com/Bilal140202/snip"
documentation = "https://bilal140202.github.io/snip/"
readme = "README.md"
keywords = ["cli", "snippets", "commands", "fuzzy", "productivity"]
categories = ["command-line-utilities", "development-tools"]
rust-version = "1.75"  # Enforce minimum Rust version

[badges]
github = { repository = "Bilal140202/snip", workflow = "CI" }
```

### 6.3 Minimum Rust Version

Set `rust-version = "1.75"` in `Cargo.toml`. This ensures:

- `cargo install` fails with a clear message on older Rust
- CI can optionally test against `rust-version` to catch accidental use of newer features
- Users know the minimum supported Rust version

### 6.4 Verify It Works

```bash
# Before publishing, verify the install experience:
cargo install --path .
snip --version
snip --help
```

---

## 7. Documentation Site

### 7.1 Strategy: mdBook on GitHub Pages

**mdBook** is the standard for Rust project documentation. It's used by the Rust book, Tokio, Clap, and many others.

### 7.2 Why mdBook

| Option | Pros | Cons |
|---|---|---|
| Single `index.html` | Zero setup | No navigation, no search, doesn't scale |
| mdBook | Rust-native, search, navigation, theming | Small learning curve |
| Docusaurus | Very polished | Node.js dependency, overkill for a CLI tool |
| Hugo/Jekyll | Flexible | Not Rust-native, config complexity |

### 7.3 Domain Suggestions

| Domain | Availability | Notes |
|---|---|---|
| `snip.sh` | Likely taken | Premium, short, memorable |
| `snip-cli.dev` | Likely available | `.dev` domains are popular for dev tools |
| `snipcli.com` | Worth checking | Clean, no hyphen |
| `getsnip.dev` | Likely available | Follows `get` prefix pattern (like `getpulumi.com`) |
| `snip.rs` | Likely taken | Short, Rust-adjacent |

**Recommendation:** `snip-cli.dev` (professional, descriptive, `.dev` is trusted by developers). Fallback: `getsnip.dev`.

### 7.4 mdBook Structure

```
docs/book/
├── book.toml
├── src/
│   ├── SUMMARY.md
│   ├── introduction.md
│   ├── installation.md
│   ├── quickstart.md
│   ├── commands/
│   │   ├── index.md
│   │   ├── add.md
│   │   ├── run.md
│   │   ├── list.md
│   │   ├── edit.md
│   │   ├── rm.md
│   │   ├── import.md
│   │   ├── init.md
│   │   ├── doctor.md
│   │   └── completions.md
│   ├── configuration.md
│   ├── snipfile-format.md
│   ├── integrating-with-editors.md
│   └── contributing.md
└── theme/                      # Optional custom theme
    └── css/
        └── custom.css
```

### 7.5 `book.toml`

```toml
[book]
title = "snip — Project-scoped Command Snippets"
authors = ["Bilal"]
language = "en"
multilingual = false
src = "src"

[build]
build-dir = "book"

[output.html]
default-theme = "light"
preferred-dark-theme = "ayu"
git-repository-url = "https://github.com/Bilal140202/snip"
edit-url-template = "https://github.com/Bilal140202/snip/edit/main/docs/book/{path}"

[output.html.search]
enable = true
limit-results = 20
```

### 7.6 GitHub Pages Deployment Workflow

```yaml
# .github/workflows/docs.yml
name: Deploy Docs

on:
  push:
    branches: [main]
    paths:
      - 'docs/book/**'

permissions:
  pages: write
  id-token: write
  contents: read

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  build-deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4

      - name: Install mdBook
        run: |
          curl -fsSL https://github.com/rust-lang/mdBook/releases/download/v0.4.40/mdbook-v0.4.40-x86_64-unknown-linux-gnu.tar.gz \
            | tar xz -C /usr/local/bin

      - name: Build book
        run: mdbook build docs/book

      - name: Setup GitHub Pages
        uses: actions/configure-pages@v5

      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: docs/book/book

      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

> The site will be available at `https://bilal140202.github.io/snip/` by default, or at a custom domain like `snip-cli.dev` after configuring DNS in the GitHub repo settings.

---

## 8. npm Wrapper Package

### 8.1 Rationale

Many developers in the Node.js ecosystem don't have Rust installed. An npm package that transparently downloads the correct binary removes the friction of:

- Installing Rust toolchain
- Running `cargo install`
- Dealing with cross-compilation

This pattern is used by `esbuild`, `turbo`, `biome`, and `oxlint`.

### 8.2 Package Structure

```
snip-cli/
├── package.json
├── install.js          # Post-install: downloads the right binary
├── bin/
│   └── snip.js         # Wrapper that spawns the real binary
├── index.js            # API export (optional, for programmatic use)
└── README.md
```

### 8.3 `package.json`

```json
{
  "name": "snip-cli",
  "version": "0.1.0",
  "description": "Project-scoped command snippets with built-in fuzzy finder",
  "bin": {
    "snip": "./bin/snip.js"
  },
  "scripts": {
    "postinstall": "node install.js",
    "prepack": "echo 'Run release workflow to populate dist/'"
  },
  "repository": {
    "type": "git",
    "url": "https://github.com/Bilal140202/snip"
  },
  "keywords": ["cli", "snippets", "commands", "fuzzy", "productivity"],
  "author": "Bilal",
  "license": "MIT",
  "engines": {
    "node": ">=18"
  },
  "os": [
    "darwin",
    "linux",
    "win32"
  ],
  "cpu": [
    "x64",
    "arm64"
  ]
}
```

### 8.4 `install.js` (post-install binary downloader)

```javascript
#!/usr/bin/env node
// install.js — Downloads the correct snip binary for the current platform

const { createRequire } = require("module");
const path = require("path");
const fs = require("fs");
const https = require("https");
const { execSync } = require("child_process");
const { pipeline } = require("stream/promises");
const { createReadStream, createWriteStream, chmodSync, mkdirSync } = fs;
const { createGunzip } = require("zlib");

const REPO = "Bilal140202/snip";
const PACKAGE_VERSION = require("./package.json").version;

function getPlatform() {
  const { platform, arch } = process;
  if (platform === "win32") return { os: "windows", arch: "x86_64", ext: "zip", bin: "snip.exe" };
  if (platform === "darwin") {
    const a = arch === "arm64" ? "aarch64" : "x86_64";
    return { os: "macos", arch: a, ext: "tar.gz", bin: "snip" };
  }
  if (platform === "linux") {
    const a = arch === "arm64" ? "aarch64" : "x86_64";
    return { os: "linux", arch: a, ext: "tar.gz", bin: "snip" };
  }
  throw new Error(`Unsupported platform: ${platform}`);
}

async function getLatestTag() {
  const url = `https://api.github.com/repos/${REPO}/releases/latest`;
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { "User-Agent": "snip-cli" } }, (res) => {
      let data = "";
      res.on("data", (chunk) => (data += chunk));
      res.on("end", () => {
        const tag = JSON.parse(data).tag_name;
        tag ? resolve(tag) : reject(new Error("Could not determine latest version"));
      });
    }).on("error", reject);
  });
}

function download(url) {
  return new Promise((resolve, reject) => {
    const file = path.join(__dirname, "download_tmp");
    https.get(url, { headers: { "User-Agent": "snip-cli" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return download(res.headers.location).then(resolve).catch(reject);
      }
      const ws = createWriteStream(file);
      res.pipe(ws);
      ws.on("finish", () => resolve(file));
    }).on("error", reject);
  });
}

async function main() {
  // Check if binary already exists (for local dev / CI)
  const platform = getPlatform();
  const binDir = path.join(__dirname, "bin-target");
  const binPath = path.join(binDir, platform.bin);

  if (fs.existsSync(binPath)) {
    try {
      execSync(`"${binPath}" --version`, { stdio: "ignore" });
      console.log("snip binary already installed.");
      return;
    } catch {}
  }

  console.log(`Downloading snip for ${platform.os}-${platform.arch}...`);

  const tag = await getLatestTag();
  const asset = `snip-${platform.arch}-${platform.os === "linux" ? "linux-musl" : platform.os}.${platform.ext}`;
  const url = `https://github.com/${REPO}/releases/download/${tag}/${asset}`;

  const tmpFile = await download(url);
  mkdirSync(binDir, { recursive: true });

  if (platform.ext === "tar.gz") {
    execSync(`tar xzf "${tmpFile}" -C "${binDir}"`, { stdio: "inherit" });
  } else {
    execSync(`unzip -o "${tmpFile}" -d "${binDir}"`, { stdio: "inherit" });
  }

  fs.unlinkSync(tmpFile);
  chmodSync(binPath, 0o755);
  console.log("snip installed successfully.");
}

main().catch((err) => {
  console.error("Failed to install snip:", err.message);
  process.exit(1);
});
```

### 8.5 `bin/snip.js` (wrapper)

```javascript
#!/usr/bin/env node
const path = require("path");
const { spawn } = require("child_process");

const platform = process.platform === "win32" ? { bin: "snip.exe" } : { bin: "snip" };
const binPath = path.join(__dirname, "..", "bin-target", platform.bin);

spawn(binPath, process.argv.slice(2), { stdio: "inherit" })
  .on("exit", (code) => process.exit(code ?? 0))
  .on("error", (err) => {
    console.error("Failed to run snip. Try: npm rebuild snip-cli");
    process.exit(1);
  });
```

### 8.6 Publishing the npm Package

The npm package should be published **after** the GitHub Release succeeds. Add to the release workflow:

```yaml
      # ── Publish to npm ──
      - name: Publish snip-cli to npm
        if: ${{ !contains(github.ref_name, '-rc') && !contains(github.ref_name, '-beta') && !contains(github.ref_name, '-alpha') }}
        working-directory: npm-package
        run: |
          npm version "${GITHUB_REF_NAME#v}" --no-git-tag-version
          npm publish --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

> **Note:** This requires an `NPM_TOKEN` secret in the repo settings, and the npm package directory to be checked in or built during CI.

---

## 9. Implementation Roadmap

### Phase 1 — Foundation (Week 1)

| Task | Priority |
|---|---|
| Replace `ci.yml` with full pipeline | P0 |
| Replace `release.yml` with full pipeline | P0 |
| Create `install.sh` in repo root | P0 |
| Add `rust-version` to `Cargo.toml` | P1 |
| Test tag-triggered release end-to-end | P0 |

### Phase 2 — Distribution (Week 2)

| Task | Priority |
|---|---|
| Create `Bilal140202/homebrew-tap` repo | P1 |
| Add formula auto-update dispatch to release.yml | P1 |
| Set up mdBook docs structure | P2 |
| Configure GitHub Pages deployment | P2 |

### Phase 3 — Ecosystem (Week 3+)

| Task | Priority |
|---|---|
| Create npm wrapper package | P2 |
| Add npm publish to release workflow | P2 |
| Publish to crates.io | P1 |
| Set up custom domain (snip-cli.dev) | P3 |
| Add coverage badge to README | P2 |

### Release Checklist (for every release)

```markdown
- [ ] Bump version in Cargo.toml
- [ ] Update CHANGELOG.md (or rely on auto-generated)
- [ ] Ensure all CI checks pass on main
- [ ] Create and push git tag: `git tag v0.X.Y && git push --tags`
- [ ] Verify GitHub Release created with all 5 binaries + checksums
- [ ] Verify Homebrew formula updated
- [ ] Verify npm package updated
- [ ] Verify docs deployed
- [ ] Verify `cargo install snip` works (after crates.io publish)
- [ ] Verify `brew install snip` works
- [ ] Verify `npm install -g snip-cli` works
- [ ] Verify `curl -fsSL ... | bash` works
- [ ] Announce release
```

---

## Summary

This design upgrades snip from a basic CI to a **production-grade release infrastructure**:

| Component | Tool | Status |
|---|---|---|
| CI (lint, test, fmt) | GitHub Actions | Designed — full YAML provided |
| Coverage | `cargo-llvm-cov` | Designed — HTML artifact upload |
| Binary size guard | Shell check, 4 MB limit | Designed |
| Cross-compilation | `cargo-zigbuild` + Zig | Designed — musl static binaries |
| Checksums | SHA256 via `sha256sum` | Designed — auto-generated |
| Install script | `install.sh` | Designed — full script provided |
| Homebrew | Tap repo + auto-update | Designed — formula + workflow |
| crates.io | `cargo publish` | Designed — metadata specified |
| Docs site | mdBook on GitHub Pages | Designed — structure + workflow |
| npm wrapper | Post-install binary download | Designed — package + scripts |
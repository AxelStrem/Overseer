#!/usr/bin/env bash
# Overseer's setup script for AgentLoop: the tools a session needs to build and check Overseer -
# Rust, Node 22 and a headless Chrome for scripts/preview.mjs - installed without root into the
# project's own folder, AGENTLOOP_PROJECT_ENV, on Linux or macOS. It leaves session-env.txt there
# for AgentLoop to give every session, and skips what an earlier run installed, so it is safe to
# run again.
#
# By hand: AGENTLOOP_PROJECT_ENV=/some/folder bash scripts/agentloop-setup.sh
set -eu

ENV_DIR="${AGENTLOOP_PROJECT_ENV:?AGENTLOOP_PROJECT_ENV names the folder to install into}"
NODE_MAJOR=22

say() { printf '%s\n' "$*"; }
fail() { printf 'setup failed: %s\n' "$*" >&2; exit 1; }

case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=darwin ;;
    *) fail "$(uname -s) is not supported: Linux or macOS" ;;
esac
case "$(uname -m)" in
    x86_64 | amd64) arch=x64 ;;
    aarch64 | arm64) arch=arm64 ;;
    *) fail "$(uname -m) is not supported: x86_64 or arm64" ;;
esac

# System packages need root, which this script never has: say what is missing instead.
missing=""
for tool in curl tar cc; do
    command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
[ -z "$missing" ] || fail "missing from the system:$missing (a C compiler and linker is build-essential on Debian)"

mkdir -p "$ENV_DIR"

# Rust, through rustup, kept in the project's folder.
export CARGO_HOME="$ENV_DIR/cargo" RUSTUP_HOME="$ENV_DIR/rustup"
if [ -x "$CARGO_HOME/bin/cargo" ]; then
    say "Rust: already installed ($("$CARGO_HOME/bin/rustc" --version))"
else
    say "Rust: installing stable"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
fi

# Node: the system's when it is new enough - in AgentLoop's container, the one the Brewfile
# installs before this script runs, since its image carries no Node; else the official build.
node_major() { "$1" -e 'process.stdout.write(process.versions.node.split(".")[0])' 2>/dev/null || echo 0; }
node_path=""
if [ -x "$ENV_DIR/node/bin/node" ]; then
    say "Node: already installed ($("$ENV_DIR/node/bin/node" --version))"
    node_path="$ENV_DIR/node/bin"
elif command -v node >/dev/null 2>&1 && [ "$(node_major node)" -ge "$NODE_MAJOR" ]; then
    say "Node: using the system's ($(node --version))"
else
    say "Node: installing $NODE_MAJOR"
    base="https://nodejs.org/dist/latest-v$NODE_MAJOR.x"
    name=$(curl -sSf "$base/SHASUMS256.txt" | awk -v want="-$os-$arch.tar.gz" 'index($2, want) { print $2; exit }')
    [ -n "$name" ] || fail "no Node $NODE_MAJOR build for $os-$arch"
    rm -rf "$ENV_DIR/node.partial"
    mkdir -p "$ENV_DIR/node.partial"
    curl -sSf "$base/$name" | tar -xz -C "$ENV_DIR/node.partial" --strip-components=1
    # Moved into place only once whole, so a run cut off midway starts over next time.
    mv "$ENV_DIR/node.partial" "$ENV_DIR/node"
    node_path="$ENV_DIR/node/bin"
fi
[ -z "$node_path" ] || export PATH="$node_path:$PATH"

# A headless Chrome for scripts/preview.mjs, which reads its path from CHROME.
browsers="$ENV_DIR/browsers"
find_chrome() { find "$browsers" -type f -name chrome-headless-shell -perm -u+x 2>/dev/null | head -n 1; }
chrome=$(find_chrome)
if [ -n "$chrome" ]; then
    say "Chrome: already installed"
else
    say "Chrome: installing chrome-headless-shell"
    # yauzl lets it unpack the download without unzip, which AgentLoop's image does not have.
    # Installed beside, then moved into place once whole: the installer refuses to go on over a
    # download a cut-off run left behind.
    rm -rf "$browsers.partial"
    npm_config_cache="$ENV_DIR/npm-cache" npx --yes -p @puppeteer/browsers -p yauzl \
        browsers install chrome-headless-shell@stable --path "$browsers.partial"
    rm -rf "$browsers"
    mv "$browsers.partial" "$browsers"
    chrome=$(find_chrome)
    [ -n "$chrome" ] || fail "chrome-headless-shell was not found after installing it"
fi
if [ "$os" = linux ] && command -v ldd >/dev/null 2>&1; then
    libs=$(ldd "$chrome" 2>/dev/null | awk '/not found/ { print $1 }' | tr '\n' ' ')
    [ -z "$libs" ] || fail "Chrome needs libraries the system lacks: $libs"
fi

# A container without unprivileged user namespaces has no sandbox for Chrome, which then refuses
# to start; preview.mjs starts it without one when PREVIEW_NO_SANDBOX says so.
no_sandbox=""
if [ "$os" = linux ]; then
    probe() { timeout 20 "$chrome" --headless --disable-gpu "$@" --dump-dom about:blank 2>&1; }
    if ! seen=$(probe); then
        if probe --no-sandbox >/dev/null; then
            no_sandbox=1
            say "Chrome: no usable sandbox here; previews run it with --no-sandbox"
        else
            fail "Chrome does not start: $(printf '%s' "$seen" | tail -n 1 | cut -c 1-300)"
        fi
    fi
fi

# What every session gets. Paths are written against {env}, so the file stays right if the data
# folder moves.
{
    say "PATH={env}/cargo/bin"
    [ -z "$node_path" ] || say "PATH={env}/node/bin"
    say "CARGO_HOME={env}/cargo"
    say "RUSTUP_HOME={env}/rustup"
    say "CHROME={env}${chrome#"$ENV_DIR"}"
    [ -z "$no_sandbox" ] || say "PREVIEW_NO_SANDBOX=1"
} > "$ENV_DIR/session-env.txt.partial"
mv "$ENV_DIR/session-env.txt.partial" "$ENV_DIR/session-env.txt"
say "Overseer's tools are set up in $ENV_DIR"

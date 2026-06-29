# syntax=docker/dockerfile:1
FROM node:22-slim

# Surfaces this image in the claude-docker launcher menu, with a friendly description.
LABEL claude.docker.menu="true" \
      claude.docker.description="PIR, Rust, C"

RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*
# poppler-utils provides pdftoppm, required by Claude Code's Read tool for PDF page rendering
RUN apt-get update && apt-get install -y curl git sudo poppler-utils && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /usr/share/keyrings/githubcli-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list > /dev/null && \
    apt-get update && apt-get install -y gh && rm -rf /var/lib/apt/lists/*
RUN useradd -m claude && echo "claude ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers
WORKDIR /workspace
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv
RUN npm install -g @anthropic-ai/claude-code
RUN mkdir -p /home/claude/.claude && chown -R claude:claude /home/claude/.claude
RUN mkdir -p /tmp/node-env && chown claude:claude /tmp/node-env

# pkg-config + llvm support cargo-fuzz (libFuzzer) and any C-linking crates;
# libssl-dev + libclang-dev are required to build ethrex (openssl-sys + bindgen).
# build-essential (above) already provides gcc/g++/ld for the "C" side.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config llvm libssl-dev libclang-dev clang && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# `host` — run any command on the host machine over SSH.
#
# The key lives at /workspace/utils/host_key (= ~/sharded-pir/utils/host_key
# on the host), so it is accessible from both sides without any extra mounts.
#
# One-time setup on the host (run as 0xalizk, only needed once):
#   ssh-keygen -t ed25519 -f ~/sharded-pir/utils/host_key -N ""
#   cat ~/sharded-pir/utils/host_key.pub >> ~/.ssh/authorized_keys
#
# Usage inside the container:
#   host systemctl status ethrex
#   host sudo systemctl stop ethrex
#   host "ulimit -n 524288 && /path/to/binary --flag"
# ---------------------------------------------------------------------------
RUN <<'EOF' tee /usr/local/bin/host
#!/bin/sh
KEY=/workspace/utils/host_key
if [ ! -f "$KEY" ]; then
    echo "host: SSH key not found at $KEY" >&2
    echo "One-time setup (on the host as 0xalizk):" >&2
    echo "  ssh-keygen -t ed25519 -f ~/sharded-pir/utils/host_key -N \"\"" >&2
    echo "  cat ~/sharded-pir/utils/host_key.pub >> ~/.ssh/authorized_keys" >&2
    exit 1
fi
# pir-ubt-node resolves to the host's public IP via /etc/hosts (propagated into container)
HOST=$(getent hosts pir-ubt-node 2>/dev/null | awk '{print $1}' | head -1)
HOST=${HOST:-172.17.0.1}
exec ssh \
    -i "$KEY" \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    -o LogLevel=ERROR \
    -o BatchMode=yes \
    "0xalizk@${HOST}" -- "$@"
EOF
RUN chmod +x /usr/local/bin/host

USER claude

# ---------------------------------------------------------------------------
# Rust toolchain for via-rs (the PIR crate).
#
# Installed per-user under /home/claude so $CARGO_HOME / $RUSTUP_HOME are
# writable at both build and run time (the container runs as `claude`).
# Matches the repo CI: stable + clippy/rustfmt for build/test/lint/doc, and
# nightly + cargo-fuzz for `just fuzz`. `just` runs the repo's Justfile recipes.
# Pinned to 1.96.0 (verified in-repo: `cargo test --workspace` -> 693 passed).
# ---------------------------------------------------------------------------
ENV PATH="/home/claude/.cargo/bin:${PATH}" \
    CARGO_TERM_COLOR=always
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y \
        --default-toolchain 1.96.0 --profile minimal \
 && rustup component add clippy rustfmt \
 && rustup toolchain install nightly --profile minimal \
 && cargo install just cargo-fuzz \
 && rm -rf /home/claude/.cargo/registry /home/claude/.cargo/git

CMD ["claude", "--dangerously-skip-permissions"]

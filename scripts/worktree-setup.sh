#!/usr/bin/env bash
set -euo pipefail

BRANCH="${1:?Usage: worktree-setup.sh <branch-name> [base-branch]}"
BASE="${2:-dev}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_DIR="$REPO_ROOT/.worktrees/$BRANCH"

# Paths that live only in the main checkout and must never be copied into a
# worktree (large, local-only media and tooling).
EXCLUDE_DIRS=(video node_modules target .dolt)

sync_agent_context() {
  local src_root="$1"
  local dst_root="$2"
  local path

  for path in skills .agents .opencode .claude; do
    [ -e "$src_root/$path" ] || continue

    if [ -e "$dst_root/$path" ]; then
      echo "agent context exists, skipping: $path" >&2
      continue
    fi

    echo "syncing agent context: $path" >&2
    if command -v rsync >/dev/null 2>&1; then
      rsync -a \
        --exclude '.git' \
        --exclude '.beads' \
        --exclude '.env' \
        --exclude '.env.*' \
        --exclude 'node_modules' \
        --exclude 'target' \
        --exclude 'dist' \
        --exclude 'build' \
        --exclude 'video' \
        --exclude '.cache' \
        "$src_root/$path" "$dst_root/"
    else
      cp -R "$src_root/$path" "$dst_root/"
    fi
  done
}

link_beads_db() {
  local src_root="$1"
  local dst_root="$2"

  [ -e "$src_root/.beads" ] || return 0

  if [ -e "$dst_root/.beads" ] || [ -L "$dst_root/.beads" ]; then
    echo "beads database exists, preserving: $dst_root/.beads" >&2
    return 0
  fi

  echo "linking canonical beads database: .beads" >&2
  ln -s "$src_root/.beads" "$dst_root/.beads"
}

link_codegraph_index() {
  local src_root="$1"
  local dst_root="$2"

  [ -e "$src_root/.codegraph" ] || return 0

  if [ -L "$dst_root/.codegraph" ]; then
    local target
    target="$(readlink "$dst_root/.codegraph")"
    if [ "$target" = "$src_root/.codegraph" ]; then
      return 0
    fi
    echo "codegraph index symlink exists, preserving: $dst_root/.codegraph -> $target" >&2
    return 0
  fi

  if [ -e "$dst_root/.codegraph" ]; then
    echo "codegraph index exists, preserving: $dst_root/.codegraph" >&2
    return 0
  fi

  echo "linking shared CodeGraph index: .codegraph" >&2
  ln -s "$src_root/.codegraph" "$dst_root/.codegraph"
}

exclude_local_worktree_paths() {
  local dst_root="$1"
  local exclude_file

  exclude_file="$(cd "$dst_root" && git rev-parse --git-path info/exclude)"
  mkdir -p "$(dirname "$exclude_file")"

  for d in .beads .codegraph "${EXCLUDE_DIRS[@]}"; do
    grep -qxF "$d" "$exclude_file" 2>/dev/null || printf "%s\n" "$d" >>"$exclude_file"
  done
}

cd "$REPO_ROOT"
mkdir -p "$(dirname "$WORKTREE_DIR")"

if [ -d "$WORKTREE_DIR" ]; then
  echo "worktree already exists: $WORKTREE_DIR" >&2
else
  git worktree add "$WORKTREE_DIR" -b "$BRANCH" "$BASE" 2>/dev/null || \
    git worktree add "$WORKTREE_DIR" "$BRANCH"
fi

link_beads_db "$REPO_ROOT" "$WORKTREE_DIR"
link_codegraph_index "$REPO_ROOT" "$WORKTREE_DIR"
exclude_local_worktree_paths "$WORKTREE_DIR"
sync_agent_context "$REPO_ROOT" "$WORKTREE_DIR"

echo "$WORKTREE_DIR"

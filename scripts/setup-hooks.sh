#!/bin/sh
# Activates the hooks in scripts/hooks for this clone.
cd "$(git rev-parse --show-toplevel)" || exit 1
chmod +x scripts/hooks/* scripts/setup-hooks.sh
git config core.hooksPath scripts/hooks
echo "hooks active: scripts/hooks"

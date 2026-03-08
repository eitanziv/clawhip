#!/bin/bash
# clawhip × OMX — Create a monitored OMX tmux session
# Usage: create.sh <session-name> <worktree-path> [channel-id] [mention]

set -euo pipefail

SESSION="${1:?Usage: $0 <session-name> <worktree-path> [channel-id] [mention]}"
WORKDIR="${2:?Usage: $0 <session-name> <worktree-path> [channel-id] [mention]}"
CHANNEL="${3:-}"
MENTION="${4:-}"

KEYWORDS="${CLAWHIP_OMX_KEYWORDS:-error,Error,FAILED,PR created,panic,complete}"
STALE_MIN="${CLAWHIP_OMX_STALE_MIN:-30}"
OMX_FLAGS="${CLAWHIP_OMX_FLAGS:---madmax}"
OMX_ENV="${CLAWHIP_OMX_ENV:-}"

if [ ! -d "$WORKDIR" ]; then
  echo "❌ Directory not found: $WORKDIR"
  exit 1
fi

# Build clawhip tmux new args
ARGS=(
  tmux new
  -s "$SESSION"
  -c "$WORKDIR"
  --keywords "$KEYWORDS"
  --stale-minutes "$STALE_MIN"
)

[ -n "$CHANNEL" ] && ARGS+=(--channel "$CHANNEL")
[ -n "$MENTION" ] && ARGS+=(--mention "$MENTION")

# Build the omx command
OMX_CMD="source ~/.zshrc"
[ -n "$OMX_ENV" ] && OMX_CMD="$OMX_CMD && $OMX_ENV"
OMX_CMD="$OMX_CMD && omx $OMX_FLAGS"

ARGS+=(-- "$OMX_CMD")

# Launch
nohup clawhip "${ARGS[@]}" &>/dev/null &

echo "✓ Created session: $SESSION in $WORKDIR (clawhip monitored)"
echo "  Monitor: tmux attach -t $SESSION"
echo "  Tail:    $(dirname "$0")/tail.sh $SESSION"

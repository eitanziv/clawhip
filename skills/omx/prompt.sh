#!/bin/bash
# clawhip × OMX — Send a prompt to an existing OMX session
# Usage: prompt.sh <session-name> "<prompt-text>"

set -euo pipefail

SESSION="${1:?Usage: $0 <session-name> \"<prompt-text>\"}"
PROMPT="${2:?Usage: $0 <session-name> \"<prompt-text>\"}"

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "❌ Session not found: $SESSION"
  exit 1
fi

# Send the prompt text followed by Enter
tmux send-keys -t "$SESSION" "$PROMPT" C-m

# Wait briefly then send another Enter to ensure TUI processes input
sleep 1
tmux send-keys -t "$SESSION" C-m

echo "✓ Sent to $SESSION (unverified): ${PROMPT:0:80}..."

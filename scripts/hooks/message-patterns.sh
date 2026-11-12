#!/bin/sh
# Shared by the commit-msg hook and CI: prints the lines of the text on stdin that the team
# message standard does not allow. Exit 0 when there is nothing to report.
tools='claude|chatgpt|gpt|(github )?copilot|gemini|cursor|codex|aider|devin|windsurf|openai|anthropic|ai|an? ai'
grep -inE "^[[:space:]]*co-authored-by:|claude-session|claude\.(ai|com)/|(^|[^a-z])generated (with|by)([[:space:]]+(\[|($tools)([^a-z]|\$))|[[:space:]]*\$)" || true

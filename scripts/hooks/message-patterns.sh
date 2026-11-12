#!/bin/sh
# Shared by the commit-msg hook and CI: prints the lines of the text on stdin that the team
# message standard does not allow. Exit 0 when there is nothing to report.
tools='claude|chatgpt|gpt|(github )?copilot|gemini|cursor|codex|aider|devin|windsurf|openai|anthropic|ai|an? ai|the ai'
# After "generated with/by": an optional colon, quotes or backticks, then a tool name that ends
# at a non-name character (a trailing full stop is fine, a file extension is not), a markdown
# link, or the end of the line.
end='([^a-z0-9_.-]|\.?$|\.[[:space:]])'
grep -inE "^[[:space:]]*co-authored-by:|claude-session|claude\.(ai|com)/|(^|[^a-z])generated (with|by)([[:space:]]*:)?[[:space:]]*([\`'\"]*(\[|($tools)$end)|\$)" || true

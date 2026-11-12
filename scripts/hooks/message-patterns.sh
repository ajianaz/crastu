#!/bin/sh
# Shared by the commit-msg hook and CI: prints the lines of the text on stdin that the team
# message standard does not allow. Exit 0 when there is nothing to report.
grep -inE '^[[:space:]]*co-authored-by:|claude-session|claude\.ai/|(^|[^a-z])generated (with|by)([[:space:]]+(\[|a |an |ai([^a-z]|$)|claude|chatgpt|gpt|copilot|gemini|cursor)|[[:space:]]*$)' || true

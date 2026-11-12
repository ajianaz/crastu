#!/bin/sh
# Shared by the commit-msg hook and CI: prints the lines of the text on stdin that the team
# message standard does not allow. Exit 0 when there is nothing to report.
#
# The text is first stripped of markdown emphasis characters and zero-width spaces, and no-break
# spaces, tabs and curly quotes become plain ones, so that
# `**Generated with** Claude` and `Generated with _Claude_` are read like the plain line.
tools='claude(-[0-9a-z.]+)*|chatgpt|gpt(-?[0-9][0-9a-z.]*)?(-[0-9a-z.]+)*|(github )?copilot|gemini|cursor|codex|aider|devin|windsurf|openai|anthropic|ai|an? ai|the ai'
# After "generated with/by": an optional colon, quotes or backticks, then a tool name that ends
# at a non-name character (a trailing full stop is fine, a file extension is not), a markdown
# link, or the end of the line.
end='([^a-z0-9_.-]|\.?$|\.[[:space:]])'
zwsp="$(printf '\342\200\213')"
nbsp="$(printf '\302\240')"
lq="$(printf '\342\200\234')"; rq="$(printf '\342\200\235')"
ls="$(printf '\342\200\230')"; rs="$(printf '\342\200\231')"
tr -d '*_' | LC_ALL=C sed -e "s/$zwsp//g" -e "s/$nbsp/ /g" -e "s/$lq/\"/g" -e "s/$rq/\"/g" -e "s/$ls/'/g" -e "s/$rs/'/g" | tr '\t' ' ' | grep -ainE "^[[:space:]]*co-authored-by:|claude-session|claude\.(ai|com)/|(^|[^a-z])generated[[:space:]]+(with|by)([[:space:]]*:)?[[:space:]]*([\`'\"]*(\[|($tools)$end)|\$)" || true

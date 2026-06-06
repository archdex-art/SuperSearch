#!/bin/sh
# SuperSearch script extension.
#
# Contract: invoked as `run.sh "<query>"`; print a JSON array of result rows to
# stdout. Each row: { title, subtitle?, action? }. Actions are executed by the
# host through the capability gate (this extension may only NetworkConnect).
#
# The query arrives as a single argv argument — never a shell string — so it is
# safe to use verbatim.

query="$1"
[ -z "$query" ] && { printf '[]'; exit 0; }

# JSON-escape the query for safe embedding in a string literal.
esc=$(printf '%s' "$query" | sed 's/\\/\\\\/g; s/"/\\"/g')
# Minimal percent-encoding for the URL (illustrative — spaces and quotes).
encoded=$(printf '%s' "$query" | sed 's/ /%20/g; s/"/%22/g')

printf '[{"title":"Search DuckDuckGo: %s","subtitle":"Open in browser","action":{"type":"open_url","url":"https://duckduckgo.com/?q=%s"}}]' \
  "$esc" "$encoded"

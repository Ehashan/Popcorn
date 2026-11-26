#!/usr/bin/env bash
# Fails if a color value appears in the UI outside ui/css/tokens.css.
set -euo pipefail

cd "$(dirname "$0")/.."
# Hex colors, and rgb()/hsl() with literal numbers instead of tokens.
pattern='#([0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b|(rgba?|hsla?)\( *[0-9]|[ :(,](white|black)\b *[;,)]'
if grep -rnE "$pattern" ui --include='*.css' --include='*.js' --include='*.html' --exclude=tokens.css; then
  echo "error: move these colors into ui/css/tokens.css" >&2
  exit 1
fi
echo "OK: all UI colors come from ui/css/tokens.css"

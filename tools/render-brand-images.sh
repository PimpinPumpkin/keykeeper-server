#!/usr/bin/env bash
# PassBundle: renders the server's own images (admin and 404 pages, email
# header, favicon) from the brand mark, the same mark the clients use
# (passbundle-clients brand/assets/mark.svg). Run after the mark or name
# changes, then commit src/static/images. Needs rsvg-convert (librsvg).
set -euo pipefail
cd "$(dirname "$0")/../src/static/images"
NAME="PassBundle"
COLOR="#1F6F5C"
MARK="M3 0h20a3 3 0 0 1 3 3v26a3 3 0 0 1-3 3H3a3 3 0 0 1-3-3V3a3 3 0 0 1 3-3Zm10 7a5 5 0 0 0-2.5 9.33L9 26h8l-1.5-9.67A5 5 0 0 0 13 7Z"
FONT="Inter, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif"

square() { # fill size out
  printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="-3 0 32 32"><path fill="%s" fill-rule="evenodd" d="%s"/></svg>' "$1" "$MARK" |
    rsvg-convert -w "$2" -h "$2" -o "$3"
}

# The navbar icon sits on a dark bar; the favicon is the brand color.
square "#FFFFFF" 64 vaultwarden-icon.png
square "$COLOR" 32 vaultwarden-favicon.png

# Email header: mark and name in dark gray, on white mail.
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 190 39"><path fill="#424242" fill-rule="evenodd" transform="translate(2 3) scale(1.03)" d="%s"/><text fill="#424242" x="36" y="28" font-family="%s" font-size="24" font-weight="700">%s</text></svg>' \
  "$MARK" "$FONT" "$NAME" | rsvg-convert -w 190 -h 39 -o logo-gray.png

# 404 page: the mark and "404".
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 500 222"><path fill="#212529" fill-rule="evenodd" transform="translate(70 31) scale(5)" d="%s"/><text fill="#212529" x="230" y="150" font-family="%s" font-size="120" font-weight="700">404</text></svg>' \
  "$MARK" "$FONT" | rsvg-convert -w 500 -h 222 -o 404.png
echo "rendered 4 images into src/static/images"

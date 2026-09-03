#!/usr/bin/env bash
# Fetches Quietust's Visual 2A03 die data and simulator into
# extern/visual2a03/, each file verified against the sha256 recorded on
# 2026-09-02 (the first fetch; docs/a0-report.md). The data derives from
# the visual6502 team's CC BY-NC-SA imagery and is never committed or
# shipped; see NOTICE.md.
set -euo pipefail
cd "$(dirname "$0")/.."

BASE="http://www.qmtpro.com/~nes/chipimages/visual2a03"
DEST="extern/visual2a03"

declare -A SHA=(
  [segdefs.js]=fe34a098ec64ee9049a0e083ec337e4d5228f0d338e1c64859c6bf2b1b2e7197
  [transdefs.js]=b10ce2c7c5b9774bb4f27476cb3e44a1ba1b290e8a463df6957803950cf44607
  [nodenames.js]=e88eeec604234012a82beb172145426c2391a06cf618c8a5f8ab32c872840679
  [wires.js]=dded17b4e9e0752a450e1f684925c2bf67ece24977006a75eaaa43325b5d6387
  [chipsim.js]=3fe3fa48a003704ef66346e042adb016c0b3bbee018e267edb4ef921ba9cc2a1
  [macros.js]=c8ec996fbad9434f84b4b829f20797df0cceae0828d87e298ca28352dfd1e8e5
  [testprogram.js]=a9b333835f100cde2ddaf01ecae5d4055a2a164495730a6c4d970a7f550974ef
  [memtable.js]=43e7cb9cf07b7b87be6bbc1ccf692fe387f23ecc7b32fb530bde7de8cb218dd0
)

mkdir -p "$DEST"
for f in "${!SHA[@]}"; do
    if [ -f "$DEST/$f" ] && echo "${SHA[$f]}  $DEST/$f" | sha256sum -c - >/dev/null 2>&1; then
        echo "already fetched: $f"
        continue
    fi
    curl -sL -o "$DEST/$f" "$BASE/$f"
    echo "${SHA[$f]}  $DEST/$f" | sha256sum -c -
done
echo "fetched and verified: $DEST"

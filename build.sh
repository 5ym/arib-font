#!/bin/sh
# フォントを元フォントから作る。Dockerfile の道具の中で動かす:
#   docker build -t denpa-font-tools . && docker run --rm -v "$PWD:/w" -e VERSION=2.0 denpa-font-tools ./build.sh
# 出力: dist/denpa-font.ttf, dist/denpa-font.woff2, dist/SHA256SUMS (build/merged.ttf は絞る前。verify.py が使う)
set -eu
cd "$(dirname "$0")"
VERSION=${VERSION:-0.0}
: "${SOURCE_DATE_EPOCH:=0}"
export SOURCE_DATE_EPOCH

S=build/src
mkdir -p "$S" dist

# 元フォント (配布元・版・sha256 を固定)
fetch() { # url sha256
  f="$S/$(basename "$1")"
  if ! echo "$2  $f" | sha256sum -c --status 2>/dev/null; then
    curl -fsSL --retry 5 -o "$f" "$1"
    echo "$2  $f" | sha256sum -c --quiet
  fi
}
# 源柔ゴシック 1.002.20150607 (http://jikasei.me/font/genjyuu/)。等幅 Regular を元にする
fetch https://ftp.iij.ad.jp/pub/osdn.jp/users/8/8636/genjyuugothic-20150607.7z \
  1997876351985ac9e2ea7a7a809c91e3fdaef878ece5b830f2512c882c3e49db
# 和田研中丸ゴシック2004ARIB (Unicode版) 4.43 (https://sourceforge.net/projects/jis2004/)。
# 源柔に無い ARIB の記号を写すのと、外字の対応表 (ARIBMAP.csv) に使う
fetch https://downloads.sourceforge.net/project/jis2004/WadaLabChuMaruGo2004ARIB/wlcmaru2004aribu4430.lzh \
  804d11ab23356cd80e03362a9d7b1005f9b2f16307a5d1ec4699f3e482285659

(cd "$S" &&
  7z x -y genjyuugothic-20150607.7z GenJyuuGothic-Monospace-Regular.ttf > /dev/null &&
  lha xqf wlcmaru2004aribu4430.lzh wlcmaru2004aribu.ttf ARIBMAP.csv)

python3 -I scripts/build.py "$S" build/merged.ttf dist/denpa-font.ttf "$VERSION"
woff2_compress dist/denpa-font.ttf > /dev/null
(cd dist && sha256sum denpa-font.ttf denpa-font.woff2 > SHA256SUMS && cat SHA256SUMS)

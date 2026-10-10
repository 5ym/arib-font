#!/bin/sh
# フォントを元フォントから作る。Dockerfile の道具の中で動かす:
#   docker build -t denpa-font-tools . && docker run --rm -v "$PWD:/w" -e VERSION=1.0 denpa-font-tools ./build.sh
# 出力: dist/denpa-font.ttf, dist/denpa-font.woff2, dist/SHA256SUMS
set -eu
cd "$(dirname "$0")"
VERSION=${VERSION:-0.0}
: "${SOURCE_DATE_EPOCH:=$(git -c safe.directory='*' log -1 --format=%ct 2>/dev/null || echo 0)}"
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
# 自家製 Rounded M+ 1.059.20150529 (http://jikasei.me/font/rounded-mplus/about.html)
fetch https://ftp.iij.ad.jp/pub/osdn.jp/users/8/8569/rounded-mplus-20150529.7z \
  e746736c8ded99fe9a9dd72a241ec59435eaa282a18e7ac33a26dc0578c06ff7
# 和田研中丸ゴシック2004ARIB (Unicode版) 4.43 (https://sourceforge.net/projects/jis2004/)
fetch https://downloads.sourceforge.net/project/jis2004/WadaLabChuMaruGo2004ARIB/wlcmaru2004aribu4430.lzh \
  804d11ab23356cd80e03362a9d7b1005f9b2f16307a5d1ec4699f3e482285659
# 和田研中丸ゴシック2004絵文字 4.73 (同上。2004ARIB に無い字だけ使う)
fetch https://downloads.sourceforge.net/project/jis2004/WadaLabChuMaruGo2004Emoji/wlcmaru2004emoji4730.zip \
  d01f401b4d158762745843e627c484c6f51d143033d34a9edebbe5472ae386c5

(cd "$S" &&
  7z x -y rounded-mplus-20150529.7z rounded-mplus-1m-regular.ttf > /dev/null &&
  lha xqf wlcmaru2004aribu4430.lzh wlcmaru2004aribu.ttf &&
  7z x -y wlcmaru2004emoji4730.zip wlcmaru2004emoji.ttf > /dev/null)

fontforge -quiet -lang=ff -script ./denpa-font.pe "$S" build/merged.ttf 2> build/fontforge.log ||
  { cat build/fontforge.log; exit 1; }
python3 -I scripts/finish.py build/merged.ttf dist/denpa-font.ttf "$VERSION"
woff2_compress dist/denpa-font.ttf > /dev/null
(cd dist && sha256sum denpa-font.ttf denpa-font.woff2 > SHA256SUMS && cat SHA256SUMS)

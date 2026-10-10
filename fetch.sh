#!/bin/sh
# 道具 (tools/) が読むものを build/src に取ってきて確かめる。道具はネットにも書庫にも触らない。
#   要るもの: curl・sha256sum・openssl・base64・7z・tar (GitHub の ubuntu ランナーには入っている)
set -eu
cd "$(dirname "$0")"
S=build/src
mkdir -p "$S/denpa"

# 字の一覧を作る denpa の表の版 (Renovate が追う)
# renovate: datasource=git-refs depName=https://github.com/danything/denpa branch=main
DENPA_COMMIT=4e71a4bede67a324edec1fae2d147e1137a40924
# EUC-JP の表 (WHATWG Encoding Standard の index-jis0208。ブラウザの TextDecoder と同じ字)
# renovate: datasource=git-refs depName=https://github.com/whatwg/encoding branch=main
WHATWG_ENCODING_COMMIT=a985b62a9b45c17da3e17a9f0a0b4e30c34c4a8a
# 源柔ゴシック 1.002.20150607 (http://jikasei.me/font/genjyuu/)。等幅 Regular を元にする
GENJYUU_URL=https://ftp.iij.ad.jp/pub/osdn.jp/users/8/8636/genjyuugothic-20150607.7z
GENJYUU_SHA256=1997876351985ac9e2ea7a7a809c91e3fdaef878ece5b830f2512c882c3e49db

# get <url> <出力> : 404 なら 1 を返す (ほかの失敗は止める)
get() {
  code=$(curl -sSL --retry 5 --retry-all-errors -o "$2.part" -w '%{http_code}' "$1")
  case "$code" in
    200) mv "$2.part" "$2" ;;
    404) rm -f "$2.part"; return 1 ;;
    *) echo "取れません ($code): $1" >&2; exit 1 ;;
  esac
}

# 元フォント
arc="$S/$(basename "$GENJYUU_URL")"
echo "$GENJYUU_SHA256  $arc" | sha256sum -c --status 2>/dev/null || get "$GENJYUU_URL" "$arc"
echo "$GENJYUU_SHA256  $arc" | sha256sum -c --quiet
7z e -y -o"$S" "$arc" GenJyuuGothic-Monospace-Regular.ttf > /dev/null

# denpa の表 (無いファイルは飛ばす。表はいずれ 1 つのファイルにまとまる)
if [ "$(cat "$S/denpa/COMMIT" 2>/dev/null)" != "$DENPA_COMMIT" ]; then
  rm -f "$S"/denpa/*
  for f in src/lib/ts/b24-tables.ts src/lib/ts/aribtext-gaiji.ts src/lib/fold.ts package.json bun.lock; do
    get "https://raw.githubusercontent.com/danything/denpa/$DENPA_COMMIT/$f" "$S/denpa/$(basename "$f")" || echo "denpa に $f はありません"
  done
  echo "$DENPA_COMMIT" > "$S/denpa/COMMIT"
fi

# データ放送の表: denpa の package.json の web-bml の版を npm から取り、bun.lock の sha512 で確かめる
ver=$(sed -n 's/.*"web-bml": *"[~^]*\([0-9][0-9.]*\)".*/\1/p' "$S/denpa/package.json" | head -1)
[ -n "$ver" ] || { echo "denpa の package.json に web-bml がありません" >&2; exit 1; }
tgz="$S/web-bml-$ver.tgz"
want=$(grep -F "\"web-bml@$ver\"" "$S/denpa/bun.lock" | sed -n 's/.*"sha512-\([^"]*\)".*/\1/p' | head -1)
[ -n "$want" ] || { echo "denpa の bun.lock に web-bml@$ver の sha512 がありません" >&2; exit 1; }
sum() { openssl dgst -sha512 -binary "$1" | base64 -w0; }
[ -f "$tgz" ] && [ "$(sum "$tgz")" = "$want" ] || get "https://registry.npmjs.org/web-bml/-/web-bml-$ver.tgz" "$tgz"
[ "$(sum "$tgz")" = "$want" ] || { echo "web-bml-$ver.tgz の sha512 が denpa の bun.lock と違います" >&2; exit 1; }
tar -xzf "$tgz" -O package/dist/client/jis_to_unicode_map.js > "$S/jis_to_unicode_map.js"

# EUC-JP の表
[ -s "$S/index-jis0208.txt" ] && grep -q "$WHATWG_ENCODING_COMMIT" "$S/index-jis0208.commit" 2>/dev/null ||
  { get "https://raw.githubusercontent.com/whatwg/encoding/$WHATWG_ENCODING_COMMIT/index-jis0208.txt" "$S/index-jis0208.txt" &&
    echo "$WHATWG_ENCODING_COMMIT" > "$S/index-jis0208.commit"; }
echo "取ってきた: 源柔ゴシック、denpa $DENPA_COMMIT、web-bml $ver、index-jis0208 $WHATWG_ENCODING_COMMIT"

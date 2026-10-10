# Denpa Font

放送の字幕・データ放送で使う字 (JIS X 0208 と ARIB の外字など) を収めた、等幅の丸ゴシックです。
自家製 Rounded M+ 1m に和田研中丸ゴシックの字を足した「Rounded M+ 1m for ARIB」(2012-12-19、2013-12-05 更新) を元にしています。

| | |
| --- | --- |
| ファミリ名 | `Denpa Font` |
| PostScript 名 | `DenpaFont-Regular` |
| em | 1024 |
| 入手 | [Releases](https://github.com/danything/denpa-font/releases) |

| ファイル | 用途 |
| --- | --- |
| `denpa-font.ttf` | TrueType (fontconfig・Android など) |
| `denpa-font.woff2` | web フォント (ブラウザ) |
| `SHA256SUMS` | 上の2つの sha256 |

旧 `rounded-mplus-1m-arib.ttf` はコミット 4cee324 まで置いてあります。

## 名前を変えた理由

元の「Rounded M+ 1m for ARIB」を入れている環境とぶつからないように、字を足して絞ったこのフォントは Denpa Font という別の名前にしました。

## 収めた字の範囲

[repertoire.txt](repertoire.txt) の 7,752 字に絞っています (元の 5.5MB → ttf 4.1MB・woff2 1.5MB)。

| 何のための字 | 出どころ |
| --- | --- |
| データ放送 | web-bml の JIS→Unicode 表と ASCII |
| 字幕 | JIS X 0208 の割り当てのある区点、外字 (85・86・90〜94 区)、かな・英数・半角記号 (denpa の `src/lib/ts/b24-tables.ts`) |
| DRCS (局が絵で送る字) | denpa が置き換える先の字 |

- U+EC00〜ECBB は除く (web-bml が自前で描く)
- 絞っても描き方は変わりません (`scripts/verify.py` が絞る前と全字を比べる)。ヒンティングは残し、縦書きと OpenType の組版 (vhea/vmtx/GSUB/GPOS) は落としています
- 元フォントのどれにも無い字は [missing.txt](missing.txt) にあります (DRCS の置き換え先の漢字 6 字: 䃯 喼 瘣 蟜 驁 麃)

一覧は denpa のチェックアウトで作り直せます。

```sh
cd denpa && bun install
bun ../denpa-font/scripts/repertoire.ts > ../denpa-font/repertoire.txt
```

## 元にしたフォントと足した字の出どころ

| | フォント | 版 | 配布元 |
| --- | --- | --- | --- |
| [1] | 自家製 Rounded M+ 1m regular | 1.059.20150529 | <http://jikasei.me/font/rounded-mplus/> |
| [2] | 和田研中丸ゴシック2004ARIB (Unicode版) | 4.43 | <https://sourceforge.net/projects/jis2004/> |
| [3] | 和田研中丸ゴシック2004絵文字 ([2] と同じ作者・同じ字形。[2] に無い字だけ使う) | 4.73 | <https://sourceforge.net/projects/jis2004/> |

配布元・版・sha256 は [build.sh](build.sh) に固定してあります。[1] を元に、無い字を [2]・[3] から足します ([1] にある字は [1] のまま)。

Rounded M+ 1m for ARIB の作者による改変 (元の `.pe` と copylist):

1. 和田丸ゴと合わせるために em を 1024 unit に変更
2. ディセントの深い gｇjｊpｐqｑyｙ を Y 方向に 90% に圧縮して +45 unit 移動
3. 半角設計の文字の一部を和田丸ゴのものに置換 ([denpa-font-copylist.txt](denpa-font-copylist.txt) の `[Detach list]`)
4. たりない文字を和田丸ゴからコピー (`[Copy list]`)
5. 外字領域に ARIB 文字を配置 (`[Refer or copy list]`)。和田丸ゴの ARIB STD-B24 93区13,14点のグリフ逆転 (No,Tel→Tel,〒) も補正
6. 表示上のアセントとディセントを縮小 (上下のダイアクリティカルマークなどが切れる場合がある)

ここで足したもの:

| 字 | 出どころ |
| --- | --- |
| ∉ ≃ ≅ ≢ ≶ ≷ ⊄ ⊅ ⊊ ⊋ ⊕ ⊖ ⊗ ⌅ ⌆ ♩ ♫ ♮ ⦅ ⦆ 嬴 誾 鏢 齕 (字幕と DRCS の置き換え先で足りなかった字) | [2] |
| ⅜ 靑 ￩ ￪ ￫ ￬ ￭ ￮ | [3] |
| U+F9C3 (互換漢字) | 統合漢字 U+907C 遼 を参照 |
| U+2212 − (字幕は JIS 1-61 をこれで出す) | 全角ハイフンマイナス U+FF0D を参照 |

## ビルドとリリースの仕方

[Dockerfile](Dockerfile) の道具 (Debian の digest と snapshot.debian.org の日付で固定) の中で [build.sh](build.sh) を動かします。同じ入力から同じバイト列ができます。

```sh
docker build -t denpa-font-tools .
docker run --rm -v "$PWD:/w" -e VERSION=1.0 denpa-font-tools ./build.sh
docker run --rm -v "$PWD:/w" denpa-font-tools \
  python3 scripts/verify.py dist/denpa-font.ttf build/merged.ttf [前の版の denpa-font.ttf]
```

`verify.py` が確かめること (PR・main・タグのたびに CI が動かす):

- 字の揃い (repertoire.txt − missing.txt が全部ある)
- 名前・em
- 絞る前後で描き方が同じ
- 前の版から描き方が変わった字は [expected-changes.txt](expected-changes.txt) にあるものだけ。字を変える PR では同じ PR で書き、リリースのあと空に戻す

repertoire.txt と denpa の表のずれは CI では見ていません。denpa の表を変えたら作り直してください。

**版**: タグは `vX.Y` (例 `v1.0`)。字を減らす・字の幅や行の高さを変えるときは X、字を足す・直すときは Y を上げます。フォントの版 (nameID 5) は `Version X.YYY`。タグを push すると GitHub Actions が作って確かめ、Releases に ttf・woff2・SHA256SUMS を置きます。

## ライセンス

このフォントと作るためのスクリプトは [MIT License](LICENSE) で配布しています。
字形は下の元フォントの作者によるもので、元フォントの許諾は次のとおりです。

### 自家製 Rounded M+ (M+ FONTS) ライセンス

```text
These fonts are free software.
Unlimited permission is granted to use, copy, and distribute them, with
or without modification, either commercially or noncommercially.
THESE FONTS ARE PROVIDED "AS IS" WITHOUT WARRANTY.
```

### 和田研中丸ゴシック2004ARIB・2004絵文字 使用条件

```text
このフォントはフリーフォントです。
いわゆるパブリックドメインと同じです。
無償で使用可能です。
商用・非商用いずれでもお使い頂けます。
このフォントは再配布が可能です。
このフォントは改変が可能です。
商用非商用に関わらずＯＳなどにインストールした状態での配布も可能です。
また、他のソフトに添付や組み込んだ形での配布も可能です。
このフォントを使用して作成したものに、このフォントを使用した旨の記載は
必要ありません。
このフォントを使用して作成したものに、このフォントの規定が継承されるこ
とはありません。
このフォントを使用や再配布をするにあたって当方への確認は必要ありません。
```

### クレジット

M+ FONTS PROJECT (M+ OUTLINE FONTS)、itouhiro (Rounded M+)、自家製フォント工房 (自家製 Rounded M+)、和田研・希土類元素レアアース (和田研中丸ゴシック)、Rounded M+ 1m for ARIB の作者

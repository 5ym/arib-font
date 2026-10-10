Denpa Font

■概要
放送の字幕・データ放送で使う字 (JIS X 0208 と ARIB の外字など) を収めた等幅の丸
ゴシックです。自家製 Rounded M+ 1m に和田研中丸ゴシックの字を足した「Rounded M+
1m for ARIB」(2012-12-19、2013-12-05 更新) を元にしています。同じ名前の元のフォン
トを入れている環境とぶつからないよう、名前を Denpa Font に変えました。

入手: https://github.com/danything/denpa-font/releases
  denpa-font.ttf    TrueType (fontconfig・Android など)
  denpa-font.woff2  web フォント (ブラウザ)
  SHA256SUMS
ファミリ名 "Denpa Font"、PostScript 名 "DenpaFont-Regular"。
旧 rounded-mplus-1m-arib.ttf はコミット 4cee324 まで置いてあります。

■収める字
repertoire.txt の字に絞っています (7,752 字。5.5MB → ttf 4.1MB・woff2 1.5MB)。
  - データ放送: web-bml の JIS→Unicode 表と ASCII
  - 字幕: JIS X 0208 の割り当てのある区点、外字 (85・86・90〜94 区)、かな・英数・
    半角記号 (denpa の src/lib/ts/b24-tables.ts)
  - denpa が DRCS (局が絵で送る字) を置き換える先の字
  - U+EC00〜ECBB は除く (web-bml が自前で描く)
絞っても描き方は変わりません (scripts/verify.py が絞る前と全字を比べる)。ヒンティ
ングは残しています。縦書きと OpenType の組版 (vhea/vmtx/GSUB/GPOS) は落としていま
す。一覧は denpa のチェックアウトで作り直せます:
  $ cd denpa && bun install && bun ../denpa-font/scripts/repertoire.ts > ../denpa-font/repertoire.txt
元フォントのどれにも無い字は missing.txt に書いてあります (今は DRCS の置き換え先の
漢字 6 字)。

■元フォント
[1]自家製 Rounded M+ 1m regular 1.059.20150529
   http://jikasei.me/font/rounded-mplus/
[2]和田研中丸ゴシック2004ARIB (Unicode版) 4.43
   https://sourceforge.net/projects/jis2004/
[3]和田研中丸ゴシック2004絵文字 4.73 ([2] と同じ作者・同じ字形。[2] に無い字だけ)
   https://sourceforge.net/projects/jis2004/
配布元・版・sha256 は build.sh に固定してあります。

■改変点
1.和田丸ゴと合わせるためにemを1024unitに変更
2.ディセントの深いgｇjｊpｐqｑyｙをY方向に90%に圧縮して+45unit移動
3.半角設計の文字の一部を和田丸ゴのものに置換 (denpa-font-copylist.txt の [Detach list])
4.たりない文字を和田丸ゴからコピー ([Copy list])。M+ にある字は M+ のまま
5.外字領域にARIB文字を配置 ([Refer or copy list])。和田丸ゴのARIB STD-B24 93区13,
  14点のグリフ逆転 (No,Tel→Tel,〒になっている) も補正
6.表示上のアセントとディセントを縮小 (→上下のダイアクリティカルマークなどが切れ
  る場合がある)
ここまでは Rounded M+ 1m for ARIB の作者によるもの (元の .pe と copylist)。以下を足
しました。
7.字幕と DRCS の置き換え先で足りなかった字を [2] から足した (∉ ⊕ ⊗ ♩ ♫ ⦅ ⦆ 嬴 など)
8.[2] に無い ⅜ 靑 ￩ ￪ ￫ ￬ ￭ ￮ を [3] から足した
9.互換漢字 U+F9C3 は統合漢字 U+907C 遼 を、MINUS SIGN U+2212 は全角ハイフンマイ
  ナス U+FF0D を参照 (JIS 1-61 を字幕は U+2212 で出す)
10.repertoire.txt の字に絞った

■作り方
Dockerfile の道具 (Debian の版と apt を日付で固定) の中で build.sh を動かします。
同じ入力から同じバイト列ができます。
  $ docker build -t denpa-font-tools .
  $ docker run --rm -v "$PWD:/w" -e VERSION=1.0 denpa-font-tools ./build.sh
  $ docker run --rm -v "$PWD:/w" denpa-font-tools python3 scripts/verify.py dist/denpa-font.ttf build/merged.ttf [前の版.ttf]
verify.py は字の揃い (repertoire.txt − missing.txt)、名前、絞る前後の描き方、前の版
からの描き方の変化を確かめます (PR・main・タグのたびに CI が動く)。字の描き方を変え
る PR では、変わる字を同じ PR で expected-changes.txt に書きます (リリースのあと空に
戻す)。repertoire.txt と denpa の表のずれは CI では見ていないので、denpa の表を変え
たら作り直してください。

■版
タグは vX.Y (例 v1.0)。字を減らす・字の幅や行の高さを変えるときは X、字を足す・直
すときは Y を上げます。フォントの版 (nameID 5) は "Version X.YYY"。タグを push す
ると GitHub Actions が作って確かめ、Releases に置きます。

■ライセンス
このフォントと作るためのスクリプトは MIT License (LICENSE) で配布しています。
字形は下の元フォントの作者によるものです。元フォントの許諾は次のとおりです。

---引用開始 自家製 Rounded M+ (M+ FONTS) ライセンス---
These fonts are free software.
Unlimited permission is granted to use, copy, and distribute them, with
or without modification, either commercially or noncommercially.
THESE FONTS ARE PROVIDED "AS IS" WITHOUT WARRANTY.
---引用終了---

---引用開始 和田研中丸ゴシック2004ARIB・2004絵文字 使用条件---
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
---引用終了---

クレジット
  M+ FONTS PROJECT (M+ OUTLINE FONTS)、itouhiro (Rounded M+)、自家製フォント工房
  (自家製 Rounded M+)、和田研・希土類元素レアアース (和田研中丸ゴシック)、
  Rounded M+ 1m for ARIB の作者

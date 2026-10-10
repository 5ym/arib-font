# 収める字

**denpa の表だけを元にします。** [fetch.sh](../fetch.sh) が [danything/denpa](https://github.com/danything/denpa) の表を版 (`DENPA_COMMIT`) を固定して取り、
道具 ([repertoire.rs](../tools/src/repertoire.rs)) が読みます。フォント側に表の写しは持ちません。TS・JS は実行せず、表のリテラルだけを読みます。

| 何のための字 | denpa の表 |
| --- | --- |
| 字幕 | `src/lib/ts/b24-tables.ts`: JIS X 0208 の割り当てのある区点、外字 85・86・90〜94 区、かな・英数・記号。漢字は EUC-JP との差分なので、EUC-JP は WHATWG の index-jis0208 (`WHATWG_ENCODING_COMMIT`) で読む |
| DRCS (局が絵で送る字) | 同じファイルの `DRCS_REPLACE` の置き換え先 |
| 番組表などの外字 | `src/lib/ts/aribtext-gaiji.ts` の `GAIJI` (計算で作るなら `EPG_ONLY`) と、検索の寄せた先 (`src/lib/fold.ts` の `FOLD`) |
| データ放送 | denpa の `package.json` の web-bml の版の `jis_to_unicode_map.js` (npm から取り、denpa の `bun.lock` の sha512 で確かめる) と ASCII |

- 表はいずれ 1 つのファイルにまとまるので、表の名前 (`HIRAGANA`・`GAIJI` など) をどのファイルからでも探す
- U+EC00〜ECBB は除く (web-bml が自前の DRCS 用フォントで描く私用領域)
- 入れられない字は [missing.txt](../missing.txt) に書く (いまは空)

## 字を足す流れ

1. denpa の表に足す → denpa の CI が「フォントに無い字」で落ちる
2. こちらの `DENPA_COMMIT` を上げる (Renovate の PR)
3. 元に無い字なら [data/parts.txt](../data/parts.txt) に組み方を足す (図形で描くなら [draw.rs](../tools/src/draw.rs))
4. タグを打って版を出し、denpa のフォントの版 (と sha256) を上げる

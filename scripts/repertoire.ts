// 収める字の一覧 (repertoire.txt) を denpa の表から作り直す。
//   cd <denpa> && bun install && bun <denpa-font>/scripts/repertoire.ts > <denpa-font>/repertoire.txt
// 入れるもの:
//   - データ放送 (web-bml の JIS→Unicode 表と ASCII)
//   - 字幕 (denpa の src/lib/ts/b24-tables.ts。かな・英数・半角記号・追加記号と、
//     JIS X 0208 の割り当てのある区点と外字の 85・86・90〜94 区)
//   - denpa が DRCS (局が絵で送る字) を置き換える先の字 (DRCS_REPLACE)
// 抜くもの: U+EC00〜ECBB (web-bml が自前の DRCS 用フォントで描く私用領域)
import { resolve } from 'node:path';

const denpa = process.cwd();
const B = await import(resolve(denpa, 'src/lib/ts/b24-tables.ts'));
const { jisToUnicodeMap } = await import(resolve(denpa, 'node_modules/web-bml/dist/client/jis_to_unicode_map.js'));

const bad = (c: number | undefined) => c === undefined || Number.isNaN(c) || c === 0xfffd || c < 0x20;
const out = new Set<number>();
const addStr = (s: string) => {
    for (const ch of s) {
        const c = ch.codePointAt(0);
        if (!bad(c)) out.add(c!);
    }
};

// データ放送
const bmlCells = new Set<number>();
(jisToUnicodeMap as (number | number[])[]).forEach((u, i) => {
    const arr = typeof u === 'number' ? [u] : u;
    if (arr.some((x) => x >= 0)) bmlCells.add(i);
    for (const x of arr) if (x >= 0 && !bad(x)) out.add(x);
});
for (let c = 0x20; c < 0x7f; c++) out.add(c);

// 字幕
for (const t of [B.HIRAGANA, B.KATAKANA, B.KANA_SYMBOLS_HALF, B.JISX0201, B.JISX0201_HALF, B.ALNUM_FULL, B.ALNUM_HALF, B.KANJI_SYMBOLS_HALF, B.ADDITIONAL]) addStr(t);
const diff = new Map<number, number>();
for (const [start, chars] of B.KANJI_DIFF as [number, string][]) {
    let i = start;
    for (const ch of chars) diff.set(i++, ch.codePointAt(0)!);
}
const euc = new TextDecoder('euc-jp');
const gaijiRow = (ku: number) => ku === 84 || ku === 85 || (ku >= 89 && ku <= 93); // 0 始まり: 85・86・90〜94 区
for (let i = 0; i < 94 * 94; i++) {
    const ku = Math.floor(i / 94);
    if (!bmlCells.has(i) && !gaijiRow(ku)) continue; // 割り当てのない区点は入れない
    const c = diff.get(i) ?? euc.decode(Uint8Array.of(ku + 0xa1, (i % 94) + 0xa1)).codePointAt(0);
    if (!bad(c)) out.add(c!);
}
out.add(0x3000);

// DRCS の置き換え先
for (const c of (B.DRCS_REPLACE as Map<string, number>).values()) out.add(c);

const list = [...out].filter((c) => c < 0xec00 || c > 0xecbb).sort((a, b) => a - b);
console.log(list.map((c) => `U+${c.toString(16).toUpperCase().padStart(4, '0')}`).join('\n'));
console.error(`${list.length} code points`);

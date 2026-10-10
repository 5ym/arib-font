"""出来たフォントを確かめる。どれかに引っかかったら終了コード 1。

    python3 scripts/verify.py dist/denpa-font.ttf build/merged.ttf [前の版の denpa-font.ttf]

1. 字の揃い: repertoire.txt の字が全部あり、空白のほかは形があること (missing.txt に書いた字を除く)。
   missing.txt の字が入ったら missing.txt から消すこと (一覧を正しく保つ)
2. 名前: ファミリ名・PostScript 名・ライセンス (nameID 13/14)、em 1024
3. 絞り込みで描き方が変わっていないこと (合成直後のフォントと全字を比べる。自動ヒンティングは除く)
4. 前の版と比べて描き方が変わった字: expected-changes.txt に書いた字だけ許す (`*` は全部)
"""

import os
import sys
import unicodedata

import freetype
import numpy as np
from fontTools.ttLib import TTFont

here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def codepoints(name):
    out = []
    path = os.path.join(here, name)
    if not os.path.exists(path):
        return out
    with open(path, encoding='utf-8') as f:
        for line in f:
            s = line.split('#')[0].strip()
            if s == '*':
                out.append(-1)  # 全部 (元を替えたときだけ)
            elif s:
                out.append(int(s.split()[0][2:], 16))
    return out


def label(cps, limit=200):
    cps = sorted(cps)
    more = f' ほか {len(cps) - limit} 字' if len(cps) > limit else ''
    return ' '.join(f'U+{c:04X}({chr(c)})' for c in cps[:limit]) + more


font_path, merged_path = sys.argv[1], sys.argv[2]
prev_path = sys.argv[3] if len(sys.argv) > 3 else None
errors = []
font = TTFont(font_path)
cmap = font.getBestCmap()
rep = set(codepoints('repertoire.txt'))
missing = set(codepoints('missing.txt'))

# 1. 字の揃い
lack = rep - missing - set(cmap)
if lack:
    errors.append(f'repertoire にあるのに無い字 {len(lack)}: {label(lack)}')
stale = missing & set(cmap)
if stale:
    errors.append(f'missing.txt にあるのに入っている字 (missing.txt から消す): {label(stale)}')
extra = set(cmap) - rep
if extra:
    errors.append(f'repertoire に無い字が入っている {len(extra)}: {label(extra)}')
print(f'字: repertoire {len(rep)}、入っている {len(rep & set(cmap))}、無い (missing.txt) {len(missing)}')

# 空の字 (形の無いグリフ) が無いこと。空白だけは空でよい。
# カラー絵文字が既定の字 (🅿 ♨ ☎ ⚡ ⛅ ❗ ⁉ など) も白黒の形を持っていること
glyf = font['glyf']
empty = {c for c, g in cmap.items() if glyf[g].numberOfContours == 0 and unicodedata.category(chr(c)) != 'Zs'}
if empty:
    errors.append(f'形の無い字: {label(empty)}')

# 2. 名前
name = font['name']
want = {1: 'Denpa Font', 2: 'Regular', 4: 'Denpa Font', 6: 'DenpaFont-Regular'}
for nid, s in want.items():
    got = name.getName(nid, 3, 1, 0x409)
    if got is None or got.toUnicode() != s:
        errors.append(f'nameID {nid} が {s!r} でない: {got}')
for nid in (0, 5, 13, 14):
    if name.getName(nid, 3, 1, 0x409) is None:
        errors.append(f'nameID {nid} が無い')
if font['head'].unitsPerEm != 1024:
    errors.append(f"em が 1024 でない: {font['head'].unitsPerEm}")
print('名前:', name.getDebugName(1), '/', name.getDebugName(6), '/', name.getDebugName(5))


# 3・4. 描いて比べる
# hinted: フォントのヒンティング (TrueType)。light: FreeType の自動ヒンティング (ブラウザが使うことがある)
MODES = {'hinted': freetype.FT_LOAD_DEFAULT, 'light': freetype.FT_LOAD_TARGET_LIGHT, 'nohint': freetype.FT_LOAD_NO_HINTING}
SIZES = (24, 36)


def render(face, c, flags, size):
    face.set_pixel_sizes(0, size)
    face.load_char(chr(c), flags | freetype.FT_LOAD_RENDER)
    g = face.glyph
    bm = g.bitmap
    canvas = np.zeros((size * 3, size * 3), np.uint8)
    if bm.rows:
        a = np.array(bm.buffer, np.uint8).reshape(bm.rows, bm.pitch)[:, :bm.width]
        x = size + g.bitmap_left
        y = size * 2 - g.bitmap_top
        canvas[y:y + a.shape[0], x:x + a.shape[1]] = a
    return canvas, g.advance.x


def differing(a_path, b_path, cps, modes=tuple(MODES)):
    fa, fb = freetype.Face(a_path), freetype.Face(b_path)
    out = set()
    for c in cps:
        for fl in (MODES[m] for m in modes):
            for size in SIZES:
                ia, aa = render(fa, c, fl, size)
                ib, ab = render(fb, c, fl, size)
                if aa != ab or not np.array_equal(ia, ib):
                    out.add(c)
                    break
            if c in out:
                break
    return out


common = sorted(set(cmap) & set(TTFont(merged_path).getBestCmap()))
# 自動ヒンティングはフォント全体の字から高さの帯を測るので、字を絞ると ²³ などが 1px 動く。
# ここではフォント自身のヒンティングとヒンティングなしで比べる
d = differing(font_path, merged_path, common, ('hinted', 'nohint'))
if d:
    errors.append(f'絞り込みで描き方が変わった字 {len(d)}: {label(list(d)[:50])}')
print(f'絞り込みの前後: {len(common)} 字を比べて違い {len(d)}')

if prev_path:
    pcmap = TTFont(prev_path).getBestCmap()
    added = set(cmap) - set(pcmap)
    removed = set(pcmap) - set(cmap)
    both = sorted(set(cmap) & set(pcmap))
    changed = differing(font_path, prev_path, both)
    allowed = set(codepoints('expected-changes.txt'))
    print(f'前の版から: 増えた字 {len(added)}、減った字 {len(removed)}、描き方が変わった字 {len(changed)}')
    if added:
        print('  増えた:', label(added))
    if removed:
        print('  減った:', label(removed))
    if changed:
        print('  変わった:', label(changed))
    if -1 not in allowed and changed - allowed:
        errors.append(f'expected-changes.txt に無いのに描き方が変わった字: {label(changed - allowed)}')
else:
    print('前の版: なし (比べない)')

for e in errors:
    print('NG:', e)
sys.exit(1 if errors else 0)

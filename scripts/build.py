"""源柔ゴシック等幅に足りない字を足し (extras.txt)、名前と日付を付け、repertoire.txt の字に絞る。

    python3 scripts/build.py <元フォントの置き場> <絞る前の ttf> <出力 ttf> <版 (例 2.0)>

- ヒンティング (fpgm/prep/cvt/gasp) は残す。外すと FreeType の描き方が変わる
- 縦書き (vhea/vmtx) と OpenType の組版 (GSUB/GPOS/GDEF) は使わないので落とす
- 行の高さは元の Rounded M+ 1m for ARIB と同じ (字幕の見え方を変えない)
- 日付は SOURCE_DATE_EPOCH (同じ入力から同じバイト列)
"""

import os
import sys

from fontTools import subset
from fontTools.misc.timeTools import epoch_diff
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

src, merged_out, out, version = sys.argv[1:5]
here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
major, minor = (int(x) for x in version.split('.'))

FAMILY = 'Denpa Font'
PS = 'DenpaFont-Regular'
COPYRIGHT = ("Copyright 2014, 2015 Adobe Systems Incorporated (http://www.adobe.com/), with Reserved Font Name 'Source'. "
             'Copyright (c) 2015 M+ FONTS PROJECT. '
             'Portions from WadaLab ChuMaruGo 2004 ARIB (WadaLab, RareEarth). '
             'Copyright (c) 2026 danything.')
LICENSE = ('This Font Software is licensed under the SIL Open Font License, Version 1.1. '
           'This license is available with a FAQ at: https://openfontlicense.org')
LICENSE_URL = 'https://openfontlicense.org'
VENDOR_URL = 'https://github.com/danything/denpa-font'
WADA_SHIFT = -120  # 和田丸ゴは源柔よりベースラインが高い

font = TTFont(f'{src}/GenJyuuGothic-Monospace-Regular.ttf', recalcTimestamp=False)
wada = TTFont(f'{src}/wlcmaru2004aribu.ttf')
for tag in ('vhea', 'vmtx', 'FFTM', 'GDEF', 'GSUB', 'GPOS', 'PfEd'):
    if tag in font:
        del font[tag]

glyf, hmtx = font['glyf'], font['hmtx']
gs = font.getGlyphSet()
cmap = dict(font.getBestCmap())
wcmap, wgs = wada.getBestCmap(), wada.getGlyphSet()


def decomposed(glyphset, name, pen):
    """部品 (composite) をばらして描く (部品の名前は持ち込まない)"""
    rec = DecomposingRecordingPen(glyphset)
    glyphset[name].draw(rec)
    rec.replay(pen)


def add_glyph(name, draw, advance):
    pen = TTGlyphPen(None)
    draw(pen)
    g = pen.glyph()
    g.recalcBounds(glyf)
    glyf[name] = g
    hmtx[name] = (advance, g.xMin if g.numberOfContours else 0)


def compose(text, pen):
    """半角の字を横に縮めて並べる。1マスに4字まで入る幅 (半角の半分) にそろえる"""
    sx, sy = 0.5, 0.8
    width = 512 * sx * len(text)
    if text.startswith('(') and not text.endswith(')'):
        x = 1024 - width  # 左半分: 右に寄せて次のマスへ続ける
    elif text.endswith(')') and not text.startswith('('):
        x = 0
    else:
        x = (1024 - width) / 2
    for ch in text:
        name = cmap[ord(ch)]
        # 字面の中心 (x 高さの真ん中あたり) を保って縦に縮める
        decomposed(gs, name, TransformPen(pen, (sx, 0, 0, sy, x, 280 * (1 - sy))))
        x += 512 * sx


added = {}
with open(os.path.join(here, 'extras.txt'), encoding='utf-8') as f:
    rows = [l.split('#')[0].split() for l in f if l.split('#')[0].strip()]
for kind, cp, *rest in rows:
    c = int(cp[2:], 16)
    if kind == 'copy':
        name = f'wada{c:04X}'
        wn = wcmap[c]
        add_glyph(name, lambda pen, wn=wn: decomposed(wgs, wn, TransformPen(pen, (1, 0, 0, 1, 0, WADA_SHIFT))), wgs[wn].width)
        added[c] = name
for kind, cp, *rest in rows:
    c = int(cp[2:], 16)
    if kind == 'alias':
        added[c] = cmap.get(int(rest[0][2:], 16)) or added[int(rest[0][2:], 16)]
    elif kind == 'compose':
        name = f'arib{c:04X}'
        add_glyph(name, lambda pen, t=rest[0]: compose(t, pen), 1024)
        added[c] = name
font.setGlyphOrder(glyf.glyphOrder)
for t in font['cmap'].tables:
    if t.isUnicode():
        for c, name in added.items():
            if c <= 0xFFFF or t.format in (12, 13):
                t.cmap[c] = name

# 名前
name = font['name']
name.names = []
for nid, s in {
    0: COPYRIGHT,
    1: FAMILY,
    2: 'Regular',
    3: f'{PS};{major}.{minor:03d}',
    4: FAMILY,
    5: f'Version {major}.{minor:03d}',
    6: PS,
    11: VENDOR_URL,
    13: LICENSE,
    14: LICENSE_URL,
}.items():
    name.setName(s, nid, 3, 1, 0x409)

# 行の高さ
os2, hhea = font['OS/2'], font['hhea']
os2.usWinAscent, os2.usWinDescent = 981, 168
hhea.ascent, hhea.descent, hhea.lineGap = 981, -168, 0
os2.sTypoLineGap = 0

head = font['head']
head.fontRevision = major + minor / 1000
head.created = head.modified = int(os.environ.get('SOURCE_DATE_EPOCH', '0')) - epoch_diff  # 1904 年起点
font.save(merged_out, reorderTables=True)

with open(os.path.join(here, 'repertoire.txt')) as f:
    unicodes = [int(l.split('#')[0].strip()[2:], 16) for l in f if l.split('#')[0].strip()]
font = TTFont(merged_out, recalcTimestamp=False)
opts = subset.Options()
opts.layout_features = []
opts.layout_closure = False
opts.notdef_outline = True
opts.hinting = True
opts.legacy_kern = False
opts.name_IDs = ['*']
opts.name_languages = ['*']
opts.name_legacy = True
opts.recalc_timestamp = False
s = subset.Subsetter(opts)
s.populate(unicodes=unicodes)
s.subset(font)
font.save(out, reorderTables=True)

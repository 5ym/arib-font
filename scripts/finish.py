"""合成したフォントに名前と日付を付け、repertoire.txt の字だけに絞る。

    python3 scripts/finish.py <合成した ttf> <出力 ttf> <版 (例 1.0)>

- ヒンティング (fpgm/prep/cvt/gasp) は残す。外すと FreeType の描き方が変わる
- 縦書き (vhea/vmtx) と OpenType の組版 (GSUB/GPOS/GDEF) は使わないので落とす
- 日付は SOURCE_DATE_EPOCH (同じ入力から同じバイト列)
"""

import os
import sys

from fontTools import subset
from fontTools.misc.timeTools import epoch_diff
from fontTools.ttLib import TTFont

src, out, version = sys.argv[1], sys.argv[2], sys.argv[3]
here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
major, minor = (int(x) for x in version.split('.'))

FAMILY = 'Denpa Font'
PS = 'DenpaFont-Regular'
VERSION = f'Version {major}.{minor:03d}'
COPYRIGHT = ('Copyright (c) 2026 danything. Based on Rounded M+ (M+ FONTS PROJECT, jikasei.me) '
             'and WadaLab ChuMaruGo 2004 ARIB/Emoji (WadaLab, RareEarth).')
LICENSE = ('This font is distributed under the MIT License. '
           'The original glyphs come from Rounded M+ and WadaLab ChuMaruGo 2004; '
           'their original terms are quoted in README.md at https://github.com/danything/denpa-font')
LICENSE_URL = 'https://github.com/danything/denpa-font/blob/main/LICENSE'
VENDOR_URL = 'https://github.com/danything/denpa-font'

font = TTFont(src, recalcTimestamp=False)

name = font['name']
name.names = []
for nid, s in {
    0: COPYRIGHT,
    1: FAMILY,
    2: 'Regular',
    3: f'{PS};{major}.{minor:03d}',
    4: FAMILY,
    5: VERSION,
    6: PS,
    11: VENDOR_URL,
    13: LICENSE,
    14: LICENSE_URL,
}.items():
    name.setName(s, nid, 3, 1, 0x409)

head = font['head']
head.fontRevision = major + minor / 1000
epoch = int(os.environ.get('SOURCE_DATE_EPOCH', '0'))
head.created = head.modified = epoch - epoch_diff  # 1904 年起点 (epoch_diff は負)

with open(os.path.join(here, 'repertoire.txt')) as f:
    unicodes = [int(l.split('#')[0].strip()[2:], 16) for l in f if l.split('#')[0].strip()]

opts = subset.Options()
opts.layout_features = []
opts.layout_closure = False
opts.notdef_outline = True
opts.hinting = True
opts.legacy_kern = False
opts.name_IDs = ['*']
opts.name_languages = ['*']
opts.name_legacy = True
opts.drop_tables += ['vhea', 'vmtx', 'FFTM', 'GDEF', 'GSUB', 'GPOS']
opts.recalc_timestamp = False
s = subset.Subsetter(opts)
s.populate(unicodes=unicodes)
s.subset(font)
font.save(out, reorderTables=True)

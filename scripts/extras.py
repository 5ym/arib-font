"""extras.txt (源柔ゴシック等幅に無い字の作り方) を作り直す。build.sh が元フォントを取ったあとで:

    python3 scripts/extras.py build/src > extras.txt

repertoire.txt のうち元 (源柔ゴシック等幅) に無い字を、次の順で埋める:
  alias    ARIB の外字の私用領域 (U+E0xx〜E3xx) は、和田研の ARIBMAP.csv が示す Unicode 5.2 の字と
           同じ字形にする (cmap で同じグリフを指す)。Unicode に無い 92区26〜31 は同じ漢字
  compose  92区56〜85 (楽器の略記。Unicode に無い) は元の半角の字を横に縮めて並べる
  copy     それでも無い字は和田研中丸ゴシック2004ARIB からそのまま写す (README に一覧)
どれでも埋まらない字は missing.txt に書く。
"""

import sys

from fontTools.ttLib import TTFont

src = sys.argv[1]
base = TTFont(f'{src}/GenJyuuGothic-Monospace-Regular.ttf').getBestCmap()
wada = TTFont(f'{src}/wlcmaru2004aribu.ttf').getBestCmap()
rep = [int(l.split('#')[0].strip()[2:], 16) for l in open('repertoire.txt') if l.split('#')[0].strip()]

# ARIB 区点 → (Unicode 5.2, 私用領域)
aribmap = {}
with open(f'{src}/ARIBMAP.csv', encoding='cp932') as f:
    for line in f.read().splitlines()[1:]:
        cols = line.split(',')
        for col in cols[2:]:
            if col and 0xE000 <= int(col, 16) <= 0xF8FF:
                aribmap[int(col, 16)] = (cols[0], int(cols[1], 16) if cols[1] else None)

# 92区26〜31: 氏 副 元 故 前 新 (和田研も漢字そのものを置いている)
KANJI = dict(zip(range(0xE290, 0xE296), '氏副元故前新'))
# 92区56〜85: 楽器の略記。2マスにまたがるものは左右に分けて描く
COMPOSE = dict(zip(range(0xE2A5, 0xE2C3), [
    '(vn)', '(ob)', '(cb)', '(ce', 'mb)', '(hp)', '(br)', '(p)', '(s)', '(ms)', '(t)', '(bs)', '(b)', '(tb)',
    '(tp)', '(ds)', '(ag)', '(eg)', '(vo)', '(fl)', '(ke', 'y)', '(sa', 'x)', '(sy', 'n)', '(or', 'g)', '(pe', 'r)']))

lines, copies, missing = [], [], []
need = [c for c in rep if c not in base]
for c in need:
    if c in wada and not 0xE000 <= c <= 0xF8FF:
        copies.append(c)
for c in need:
    if 0xE000 <= c <= 0xF8FF:
        ku, u = aribmap[c]
        if c in KANJI:
            lines.append(f'alias   U+{c:04X} U+{ord(KANJI[c]):04X}  # {ku} {KANJI[c]}')
        elif c in COMPOSE:
            lines.append(f'compose U+{c:04X} {COMPOSE[c]}  # {ku}')
        elif u is not None and (u in base or u in copies):
            lines.append(f'alias   U+{c:04X} U+{u:04X}  # {ku} {chr(u)}')
        else:
            missing.append(c)
    elif c in copies:
        lines.append(f'copy    U+{c:04X}  # {chr(c)}')
    else:
        missing.append(c)

print('# 源柔ゴシック等幅に無い字の作り方 (scripts/extras.py が作る。手で直さない)')
print('# alias U+私用 U+字: 同じ字形 / compose U+私用 文字列: 半角の字を縮めて並べる / copy U+字: 和田研中丸ゴシック2004ARIB から写す')
print('\n'.join(lines))
for c in missing:
    print(f'# missing U+{c:04X} {chr(c)}', file=sys.stderr)

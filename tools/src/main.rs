//! Denpa Font を作る道具。リポジトリの根で動かす:
//!
//!   cargo run --release --manifest-path tools/Cargo.toml -- build 2.1      元から作る (dist/。build/ に途中のもの)
//!   cargo run ... -- verify dist/denpa-font.ttf build/merged.ttf [前の版]   確かめる
//!   cargo run ... -- repertoire                                            収める字の一覧を見る

mod draw;
mod repertoire;
mod sfnt;
mod sources;
mod verify;

use anyhow::{bail, Context, Result};
use repertoire::Pua;
use sfnt::{put_i16, put_u16, Glyphs};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use write_fonts::read::{FontRef, TableProvider};
use write_fonts::types::{GlyphId, NameId, Tag};

const SRC: &str = "build/src";
const FAMILY: &str = "Denpa Font";
const PS: &str = "DenpaFont-Regular";
const COPYRIGHT: &str = "Copyright 2014, 2015 Adobe Systems Incorporated (http://www.adobe.com/), with Reserved Font Name 'Source'. \
Copyright (c) 2015 M+ FONTS PROJECT. Copyright (c) 2026 danything.";
const LICENSE: &str = "This Font Software is licensed under the SIL Open Font License, Version 1.1. \
This license is available with a FAQ at: https://openfontlicense.org";
const LICENSE_URL: &str = "https://openfontlicense.org";
const VENDOR_URL: &str = "https://github.com/danything/denpa-font";
/// 行の高さ (元の Rounded M+ 1m for ARIB と同じ。字幕の見え方を変えない)
const WIN_ASCENT: u16 = 981;
const WIN_DESCENT: u16 = 168;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("build") => build(args.get(1).map_or("0.0", String::as_str)),
        Some("verify") => {
            if args.len() < 3 {
                bail!("verify <ttf> <絞る前の ttf> [前の版の ttf]");
            }
            verify::run(Path::new(&args[1]), Path::new(&args[2]), args.get(3).map(Path::new))
        }
        Some("repertoire") => repertoire::run(Path::new(SRC)),
        _ => bail!("build <版> | verify <ttf> <merged> [prev] | repertoire"),
    }
}

/// 1行1字の一覧 (U+XXXX。# から後は説明)
pub fn read_codepoints(path: &str) -> Result<Vec<u32>> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut out = vec![];
    for line in text.lines() {
        let s = line.split('#').next().unwrap().trim();
        if s.is_empty() {
            continue;
        }
        if s == "*" {
            out.push(u32::MAX); // 全部 (元を替えたときだけ)
            continue;
        }
        let h = s.split_whitespace().next().unwrap().trim_start_matches("U+");
        out.push(u32::from_str_radix(h, 16).with_context(|| format!("{path}: {line}"))?);
    }
    Ok(out)
}

fn build(version: &str) -> Result<()> {
    let (major, minor) = version.split_once('.').context("版は X.Y")?;
    let (major, minor): (u16, u16) = (major.parse()?, minor.parse()?);
    let epoch: i64 = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let src = Path::new(SRC);
    sources::fetch(&sources::GENJYUU, src)?;
    let gb = fs::read(src.join("GenJyuuGothic-Monospace-Regular.ttf"))?;
    let base = FontRef::new(&gb)?;
    if base.head()?.units_per_em() != 1024 {
        bail!("em が 1024 でない (描く数値は 1024 の前提)");
    }
    let repertoire = repertoire::load(src)?;
    let mut glyphs = Glyphs::read(&base)?;
    let mut cmap = sfnt::charmap(&base);

    // 元に無い字: 描く → 私用領域は同じ区点の字へ (無ければ文字列を縮めて並べる)。作り方は build/extras.txt に書き出す
    let mut report = String::from("# 源柔ゴシック等幅に無い字の作り方 (tools の build が書く)\n");
    let mut added: BTreeMap<u32, u32> = BTreeMap::new();
    let need: Vec<u32> = repertoire.codepoints.iter().copied().filter(|c| !cmap.contains_key(c)).collect();
    let ch = |c: u32| char::from_u32(c).unwrap_or('?');
    for &c in &need {
        if repertoire.pua.contains_key(&c) {
            continue;
        }
        if let Some(contours) = draw::draw(c, &glyphs, &cmap)? {
            added.insert(c, glyphs.add(&contours, 1024));
            writeln!(report, "draw    U+{c:04X}  # {}", ch(c))?;
        }
    }
    for &c in &need {
        match repertoire.pua.get(&c) {
            Some(Pua::Char(t)) => match cmap.get(t).or(added.get(t)) {
                Some(&g) => {
                    added.insert(c, g);
                    writeln!(report, "alias   U+{c:04X} U+{t:04X}  # {}", ch(*t))?;
                }
                None => writeln!(report, "# missing U+{c:04X} (→ U+{t:04X} {} も無い)", ch(*t))?,
            },
            Some(Pua::Text(t)) => {
                added.insert(c, glyphs.add(&draw::compose(&glyphs, &cmap, t)?, 1024));
                writeln!(report, "compose U+{c:04X} {t}")?;
            }
            None if added.contains_key(&c) => {}
            None => writeln!(report, "# missing U+{c:04X} {}", ch(c))?,
        }
    }
    cmap.extend(added);
    fs::create_dir_all("build")?;
    fs::write("build/extras.txt", &report)?;
    let rep: Vec<u32> = repertoire.codepoints.iter().copied().collect();
    fs::write("build/repertoire.txt", rep.iter().map(|c| format!("U+{c:04X}\n")).collect::<String>())?;

    // 表を組む (元の表は運び、変えるものだけ作り直す)
    let mut b = write_fonts::FontBuilder::new();
    let drop = [b"vhea", b"vmtx", b"FFTM", b"GDEF", b"GSUB", b"GPOS", b"PfEd", b"DSIG"];
    let rewrite = [b"glyf", b"loca", b"hmtx", b"maxp", b"head", b"hhea", b"OS/2", b"cmap", b"name", b"post"];
    for rec in base.table_directory().table_records() {
        let t = rec.tag();
        if drop.iter().chain(rewrite.iter()).any(|x| Tag::new(x) == t) {
            continue;
        }
        b.add_raw(t, sfnt::table(&base, &t.to_be_bytes())?.to_vec());
    }
    let (glyf, loca, hmtx) = glyphs.tables();
    b.add_raw(Tag::new(b"glyf"), glyf);
    b.add_raw(Tag::new(b"loca"), loca);
    b.add_raw(Tag::new(b"hmtx"), hmtx);
    let n = glyphs.len() as u16;
    let mut maxp = sfnt::table(&base, b"maxp")?.to_vec();
    let (mp, mc) = glyphs.max_points_contours();
    put_u16(&mut maxp, 4, n);
    let (mp, mc) = (mp.max(sfnt::u16_at(&maxp, 6)), mc.max(sfnt::u16_at(&maxp, 8)));
    put_u16(&mut maxp, 6, mp);
    put_u16(&mut maxp, 8, mc);
    b.add_raw(Tag::new(b"maxp"), maxp);
    let mut head = sfnt::table(&base, b"head")?.to_vec();
    let rev = ((major as f64 + minor as f64 / 1000.0) * 65536.0).round() as i32;
    head[4..8].copy_from_slice(&rev.to_be_bytes());
    let mac = epoch + 2_082_844_800; // 1904 年起点
    head[20..28].copy_from_slice(&mac.to_be_bytes());
    head[28..36].copy_from_slice(&mac.to_be_bytes());
    put_i16(&mut head, 50, 1);
    b.add_raw(Tag::new(b"head"), head);
    let mut hhea = sfnt::table(&base, b"hhea")?.to_vec();
    put_i16(&mut hhea, 4, WIN_ASCENT as i16);
    put_i16(&mut hhea, 6, -(WIN_DESCENT as i16));
    put_i16(&mut hhea, 8, 0);
    put_u16(&mut hhea, 34, n);
    b.add_raw(Tag::new(b"hhea"), hhea);
    let mut os2 = sfnt::table(&base, b"OS/2")?.to_vec();
    put_i16(&mut os2, 72, 0);
    put_u16(&mut os2, 74, WIN_ASCENT);
    put_u16(&mut os2, 76, WIN_DESCENT);
    b.add_raw(Tag::new(b"OS/2"), os2);
    let mut post = sfnt::table(&base, b"post")?[..32].to_vec();
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes()); // 字の名前は持たない
    b.add_raw(Tag::new(b"post"), post);
    let cm = write_fonts::tables::cmap::Cmap::from_mappings(
        cmap.iter().filter_map(|(&c, &g)| char::from_u32(c).map(|ch| (ch, GlyphId::new(g)))),
    )
    .map_err(|e| anyhow::anyhow!("cmap: {e:?}"))?;
    b.add_table(&cm)?;
    let names = [
        (0, COPYRIGHT.to_string()),
        (1, FAMILY.to_string()),
        (2, "Regular".to_string()),
        (3, format!("{PS};{major}.{minor:03}")),
        (4, FAMILY.to_string()),
        (5, format!("Version {major}.{minor:03}")),
        (6, PS.to_string()),
        (11, VENDOR_URL.to_string()),
        (13, LICENSE.to_string()),
        (14, LICENSE_URL.to_string()),
    ];
    let records = names
        .into_iter()
        .map(|(id, s)| write_fonts::tables::name::NameRecord::new(3, 1, 0x409, NameId::new(id), s.into()))
        .collect();
    b.add_table(&write_fonts::tables::name::Name::new(records))?;
    let merged = b.build();
    fs::create_dir_all("dist")?;
    fs::write("build/merged.ttf", &merged)?;

    // denpa の表の字に絞る。ヒンティングは残し、組版の表と縦書きは落とす
    let font = FontRef::new(&merged)?;
    let ttf = fix_extents(&subset(&font, &rep)?)?;
    fs::write("dist/denpa-font.ttf", &ttf)?;
    let woff2 = ttf2woff2::encode(&ttf, ttf2woff2::BrotliQuality::from(11u8)).map_err(|e| anyhow::anyhow!("woff2: {e}"))?;
    fs::write("dist/denpa-font.woff2", &woff2)?;
    let sums = format!(
        "{}  denpa-font.ttf\n{}  denpa-font.woff2\n",
        sources::sha256_hex(&ttf),
        sources::sha256_hex(&woff2)
    );
    fs::write("dist/SHA256SUMS", &sums)?;
    print!("{sums}");
    Ok(())
}

pub fn subset(font: &FontRef, unicodes: &[u32]) -> Result<Vec<u8>> {
    use skera::{subset_font, Plan, SubsetFlags};
    use write_fonts::read::collections::IntSet;
    let gids = IntSet::empty();
    let mut u = IntSet::empty();
    for &c in unicodes {
        u.insert(c);
    }
    let flags = SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
        | SubsetFlags::SUBSET_FLAGS_NO_LAYOUT_CLOSURE
        | SubsetFlags::SUBSET_FLAGS_NO_BIDI_CLOSURE; // 鏡像の字 (∼ など) を勝手に足さない
    let mut drop: IntSet<Tag> = IntSet::empty();
    for t in [
        b"morx", b"mort", b"kerx", b"kern", b"JSTF", b"DSIG", b"EBDT", b"EBLC", b"EBSC", b"SVG ", b"PCLT", b"LTSH", b"feat",
        b"Glat", b"Gloc", b"Silf", b"Sill", b"vhea", b"vmtx", b"GDEF", b"GSUB", b"GPOS",
    ] {
        drop.insert(Tag::new(t));
    }
    let mut scripts: IntSet<Tag> = IntSet::empty();
    scripts.invert();
    let features: IntSet<Tag> = IntSet::empty();
    let mut name_ids: IntSet<NameId> = IntSet::empty();
    name_ids.invert();
    let mut langs: IntSet<u16> = IntSet::empty();
    langs.invert();
    let plan = Plan::new(&gids, &u, font, flags, &drop, &scripts, &features, &name_ids, &langs);
    subset_font(font, &plan).map_err(|e| anyhow::anyhow!("絞り込み: {e}"))
}

/// 絞ったあとの字で head の外枠と hhea の余白の最小・最大を測り直す (絞り込みは元のフォント全体の値のまま残す)
fn fix_extents(ttf: &[u8]) -> Result<Vec<u8>> {
    let font = FontRef::new(ttf)?;
    let g = Glyphs::read(&font)?;
    let (mut x0, mut y0, mut x1, mut y1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    let (mut min_lsb, mut min_rsb, mut max_ext) = (i16::MAX, i16::MAX, i16::MIN);
    for (i, d) in g.data.iter().enumerate() {
        if d.is_empty() {
            continue;
        }
        let (a, b, c, e) = (sfnt::i16_at(d, 2), sfnt::i16_at(d, 4), sfnt::i16_at(d, 6), sfnt::i16_at(d, 8));
        (x0, y0, x1, y1) = (x0.min(a), y0.min(b), x1.max(c), y1.max(e));
        let ext = g.lsb[i] + (c - a);
        min_lsb = min_lsb.min(g.lsb[i]);
        min_rsb = min_rsb.min(g.advance[i] as i16 - ext);
        max_ext = max_ext.max(ext);
    }
    let mut b = write_fonts::FontBuilder::new();
    for rec in font.table_directory().table_records() {
        let t = rec.tag();
        let mut data = sfnt::table(&font, &t.to_be_bytes())?.to_vec();
        if t == Tag::new(b"head") {
            for (o, v) in [(36, x0), (38, y0), (40, x1), (42, y1)] {
                put_i16(&mut data, o, v);
            }
        } else if t == Tag::new(b"hhea") {
            for (o, v) in [(12, min_lsb), (14, min_rsb), (16, max_ext)] {
                put_i16(&mut data, o, v);
            }
        }
        b.add_raw(t, data);
    }
    Ok(b.build())
}

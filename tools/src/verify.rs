//! 出来たフォントを確かめる。どれかに引っかかったら終了コード 1。
//!
//! 1. 字の揃い: denpa の表から作った字 (build/repertoire.txt) が全部あり (missing.txt の字を除く)、空白のほかは形があること
//!    (カラー絵文字が既定の字も白黒の形を持つ)。missing.txt の字が入ったら missing.txt から消す
//! 2. 名前・em
//! 3. 絞り込みで字が変わっていないこと: 合成直後のフォントと全字を、ヒンティング無し・フォント自身の
//!    ヒンティング (TrueType の命令) で描いた輪郭 (24・36px) と送り幅で比べる
//!    (自動ヒンティングはフォント全体の字から高さの帯を測るので、字を絞ると ²³ などが動く。ここでは比べない)
//! 4. 前の版と比べて描き方が変わった字: 3 に自動ヒンティング (FreeType の light 相当) も足して比べ、
//!    expected-changes.txt に書いた字だけ許す (`*` は全部)
//! 5. woff2 を解いたもの (CI が woff2_decompress で解く) が ttf と同じ字になること

use crate::read_codepoints;
use crate::err::Result;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn label(cps: &BTreeSet<u32>) -> String {
    let limit = 200;
    let mut s: Vec<String> = cps
        .iter()
        .take(limit)
        .map(|&c| format!("U+{c:04X}({})", char::from_u32(c).unwrap_or('?')))
        .collect();
    if cps.len() > limit {
        s.push(format!("ほか {} 字", cps.len() - limit));
    }
    s.join(" ")
}

#[derive(Default)]
struct Rec(Vec<i64>);
impl OutlinePen for Rec {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.extend([0, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.extend([1, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.extend([2, (cx * 64.0) as i64, (cy * 64.0) as i64, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.0.extend([3, (a * 64.0) as i64, (b * 64.0) as i64, (c * 64.0) as i64, (d * 64.0) as i64, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn close(&mut self) {
        self.0.push(4);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Unhinted,
    Interpreter,
    Auto,
}

/// 字ごとの「描いた輪郭 + 送り幅」(大きさ・描き方ごと)
fn drawings(data: &[u8], cps: &[u32], modes: &[Mode]) -> Result<BTreeMap<u32, Vec<Vec<i64>>>> {
    let font = FontRef::new(data)?;
    let charmap = font.charmap();
    let outlines = font.outline_glyphs();
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut out: BTreeMap<u32, Vec<Vec<i64>>> = BTreeMap::new();
    let mut insts = vec![];
    for &ppem in &[24.0f32, 36.0] {
        for &m in modes {
            let inst = match m {
                Mode::Unhinted => None,
                Mode::Interpreter => Some(HintingInstance::new(
                    &outlines,
                    Size::new(ppem),
                    LocationRef::default(),
                    HintingOptions { engine: Engine::Interpreter, target: SmoothMode::Normal.into() },
                )?),
                Mode::Auto => Some(HintingInstance::new(
                    &outlines,
                    Size::new(ppem),
                    LocationRef::default(),
                    HintingOptions { engine: Engine::Auto(None), target: SmoothMode::Light.into() },
                )?),
            };
            insts.push((ppem, inst));
        }
    }
    for &c in cps {
        let Some(gid) = charmap.map(c) else { continue };
        let glyph = outlines.get(gid).ok_or_else(|| format!("glyph {gid} がありません"))?;
        let mut v = vec![vec![metrics.advance_width(gid).unwrap_or(0.0) as i64]];
        for (ppem, inst) in &insts {
            let mut pen = Rec::default();
            let settings = match inst {
                None => DrawSettings::unhinted(Size::new(*ppem), LocationRef::default()),
                Some(i) => DrawSettings::hinted(i, false),
            };
            glyph.draw(settings, &mut pen)?;
            v.push(pen.0);
        }
        out.insert(c, v);
    }
    Ok(out)
}

fn differing(a: &[u8], b: &[u8], cps: &[u32], modes: &[Mode]) -> Result<BTreeSet<u32>> {
    let da = drawings(a, cps, modes)?;
    let db = drawings(b, cps, modes)?;
    Ok(cps.iter().copied().filter(|c| da.get(c) != db.get(c)).collect())
}

pub fn run(font_path: &Path, merged_path: &Path, prev_path: Option<&Path>, woff2_ttf: Option<&Path>) -> Result<()> {
    let data = std::fs::read(font_path)?;
    let merged = std::fs::read(merged_path)?;
    let font = FontRef::new(&data)?;
    let mut errors: Vec<String> = vec![];
    let cmap: BTreeMap<u32, GlyphId> = font.charmap().mappings().collect();
    let have: BTreeSet<u32> = cmap.keys().copied().collect();
    let rep: BTreeSet<u32> = read_codepoints("build/repertoire.txt")?.into_iter().collect();
    let missing: BTreeSet<u32> = read_codepoints("missing.txt")?.into_iter().collect();

    // 1. 字の揃い
    let lack: BTreeSet<u32> = rep.difference(&missing).filter(|c| !have.contains(c)).copied().collect();
    if !lack.is_empty() {
        errors.push(format!("repertoire にあるのに無い字 {}: {}", lack.len(), label(&lack)));
    }
    let stale: BTreeSet<u32> = missing.intersection(&have).copied().collect();
    if !stale.is_empty() {
        errors.push(format!("missing.txt にあるのに入っている字 (missing.txt から消す): {}", label(&stale)));
    }
    let extra: BTreeSet<u32> = have.difference(&rep).copied().collect();
    if !extra.is_empty() {
        errors.push(format!("repertoire に無い字が入っている {}: {}", extra.len(), label(&extra)));
    }
    println!("字: repertoire {}、入っている {}、無い (missing.txt) {}", rep.len(), rep.intersection(&have).count(), missing.len());
    let outlines = font.outline_glyphs();
    let mut empty = BTreeSet::new();
    for (&c, &gid) in &cmap {
        let space = char::from_u32(c).is_some_and(|ch| ch.is_whitespace());
        let mut pen = Rec::default();
        if let Some(g) = outlines.get(gid) {
            g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)?;
        }
        if pen.0.is_empty() && !space {
            empty.insert(c);
        }
    }
    if !empty.is_empty() {
        errors.push(format!("形の無い字: {}", label(&empty)));
    }

    // 2. 名前・em
    let get = |id: skrifa::string::StringId| font.localized_strings(id).english_or_first().map(|s| s.to_string());
    use skrifa::string::StringId as S;
    for (id, want) in [(S::FAMILY_NAME, "Denpa Font"), (S::SUBFAMILY_NAME, "Regular"), (S::FULL_NAME, "Denpa Font"), (S::POSTSCRIPT_NAME, "DenpaFont-Regular")] {
        if get(id).as_deref() != Some(want) {
            errors.push(format!("nameID {} が {want:?} でない: {:?}", id.to_u16(), get(id)));
        }
    }
    for id in [S::COPYRIGHT_NOTICE, S::VERSION_STRING, S::LICENSE_DESCRIPTION, S::LICENSE_URL] {
        if get(id).is_none() {
            errors.push(format!("nameID {} が無い", id.to_u16()));
        }
    }
    let upem = font.head()?.units_per_em();
    if upem != 1024 {
        errors.push(format!("em が 1024 でない: {upem}"));
    }
    println!(
        "名前: {} / {} / {}",
        get(S::FAMILY_NAME).unwrap_or_default(),
        get(S::POSTSCRIPT_NAME).unwrap_or_default(),
        get(S::VERSION_STRING).unwrap_or_default()
    );

    // 3. 絞り込みの前後
    let cps: Vec<u32> = have.iter().copied().collect();
    let d = differing(&data, &merged, &cps, &[Mode::Unhinted, Mode::Interpreter])?;
    if !d.is_empty() {
        errors.push(format!("絞り込みで描き方が変わった字 {}: {}", d.len(), label(&d)));
    }
    println!("絞り込みの前後: {} 字を比べて違い {}", cps.len(), d.len());

    // 4. 前の版
    if let Some(p) = prev_path {
        let prev = std::fs::read(p)?;
        let pfont = FontRef::new(&prev)?;
        let pset: BTreeSet<u32> = pfont.charmap().mappings().map(|(c, _)| c).collect();
        let added: BTreeSet<u32> = have.difference(&pset).copied().collect();
        let removed: BTreeSet<u32> = pset.difference(&have).copied().collect();
        let both: Vec<u32> = have.intersection(&pset).copied().collect();
        let changed = differing(&data, &prev, &both, &[Mode::Unhinted, Mode::Interpreter, Mode::Auto])?;
        let allowed: BTreeSet<u32> = read_codepoints("expected-changes.txt")?.into_iter().collect();
        println!("前の版から: 増えた字 {}、減った字 {}、描き方が変わった字 {}", added.len(), removed.len(), changed.len());
        for (name, set) in [("増えた", &added), ("減った", &removed), ("変わった", &changed)] {
            if !set.is_empty() {
                println!("  {name}: {}", label(set));
            }
        }
        let bad: BTreeSet<u32> = changed.difference(&allowed).copied().collect();
        if !allowed.contains(&u32::MAX) && !bad.is_empty() {
            errors.push(format!("expected-changes.txt に無いのに描き方が変わった字: {}", label(&bad)));
        }
    } else {
        println!("前の版: なし (比べない)");
    }

    // 5. woff2 (解くのは道具の外。CI が woff2_decompress で解いたものを渡す)
    if let Some(p) = woff2_ttf {
        let back = std::fs::read(p)?;
        let d = differing(&data, &back, &cps, &[Mode::Unhinted, Mode::Interpreter])?;
        if !d.is_empty() {
            errors.push(format!("woff2 を解くと違う字 {}: {}", d.len(), label(&d)));
        }
        println!("woff2: 解いて {} 字を比べて違い {}", cps.len(), d.len());
    }

    for e in &errors {
        println!("NG: {e}");
    }
    if !errors.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

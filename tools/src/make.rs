//! 字を 1 つずつ作る。
//!
//! - 元の字 (BIZ UDゴシック): 輪郭のまま角に円弧を入れる (fillet.rs)
//! - 描いた字 (draw.rs) と部品を組む字 (parts.txt): 図形や部品が重なるので、格子に塗って丸め (round.rs)、
//!   輪郭に当てはめる (fit.rs)
//!
//! 字どうしは関わらないので、字ごとに別のスレッドで作る (どのスレッドで作っても同じバイト列になる)。

use crate::draw;
use crate::err::Result;
use crate::fillet;
use crate::fit;
use crate::parts::{Part, Place, Source, STEM};
use crate::round::{self, Grid, Poly};
use crate::sfnt::{self, Contour};
use std::collections::BTreeMap;

pub type Shape = Vec<Contour>;
/// 元の字 (BIZ UDゴシックを em 1024 にしたもの): Unicode → (輪郭, 送り幅)
pub type Base = BTreeMap<u32, (Shape, u16)>;

pub enum Job {
    /// 輪郭をそのまま (丸めない字・空白)
    Keep(Shape),
    /// 元の字: 輪郭の角を丸める
    Fillet(Shape),
    /// 描いた字: 格子に塗って丸める
    Raster(Shape),
    /// 部品を組んで丸める (true なら組むだけで丸めない)
    Parts(Vec<Part>, bool),
}

/// 折れ線にするときの許し
const FLAT_TOL: f64 = 0.1;

pub fn bbox(polys: &[Poly]) -> Option<(f64, f64, f64, f64)> {
    let mut it = polys.iter().flatten().peekable();
    it.peek()?;
    let (mut a, mut b, mut c, mut d) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in it {
        (a, b, c, d) = (a.min(x), b.min(y), c.max(x), d.max(y));
    }
    Some((a, b, c, d))
}

fn union(a: Option<(f64, f64, f64, f64)>, b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    match a {
        None => b,
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
    }
}

/// 字を作って glyf の 1 字分のバイト列にする。2 つめは、元の字を輪郭のまま丸めると輪郭が交わるので
/// 格子で丸めたとき true
pub fn make(job: &Job, base: &Base) -> Result<(Vec<u8>, bool)> {
    let contours = match job {
        Job::Keep(s) => return Ok((sfnt::encode_simple(s), false)),
        Job::Fillet(s) => match fillet::round_shape(s) {
            Some(c) => c,
            None => return Ok((sfnt::encode_simple(&raster(s)), true)),
        },
        Job::Raster(s) => raster(s),
        Job::Parts(parts, sharp) => {
            let (g, mask) = compose(parts, base)?;
            trace(&g, &mask, *sharp)
        }
    };
    Ok((sfnt::encode_simple(&contours), false))
}

/// 輪郭を格子に塗って丸める
fn raster(s: &Shape) -> Shape {
    let polys = round::flatten(s, FLAT_TOL);
    let Some(b) = bbox(&polys) else { return vec![] };
    let g = Grid::covering(b, round::R1 + 4.0);
    let mask = g.fill(&polys);
    trace(&g, &mask, false)
}

/// 塗った字を丸めて輪郭にする
fn trace(g: &Grid, mask: &[bool], sharp: bool) -> Shape {
    round::round_mask(g, mask, sharp).iter().filter_map(fit::fit).collect()
}

/// 部品の元の輪郭
fn source(src: &Source, base: &Base) -> Result<Shape> {
    Ok(match *src {
        Source::Char(c) => base.get(&c).ok_or_else(|| format!("BIZ UDゴシックに U+{c:04X} がありません"))?.0.clone(),
        Source::Ring(cx, cy, r, w) => draw::ring(cx, cy, r, w),
        Source::Disc(cx, cy, r) => draw::circle(cx, cy, r),
        Source::Frame(x0, y0, x1, y1, w) => draw::frame(x0, y0, x1, y1, 0.0, w),
        Source::Rect(x0, y0, x1, y1) => draw::rrect(x0, y0, x1, y1, 0.0),
        Source::Text(_) => return Err("字の並びは先にばらす".into()),
    })
}

struct Placed {
    sub: bool,
    polys: Vec<Poly>,
    crop: Option<Poly>,
    /// 横・縦に膨らませる量 (負なら縮める)
    grow: (f64, f64),
}

/// 字の並び (Source::Text) を、1 字ずつの部品にばらす。墨の幅で詰め (字の間は TEXT_GAP)、
/// 基線から TEXT_BAND の高さを縦横同じ比で置き方の枠に収める
fn expand(parts: &[Part], base: &Base) -> Result<Vec<Part>> {
    const TEXT_BAND: (f64, f64) = (-30.0, 830.0);
    const TEXT_GAP: f64 = 90.0;
    let mut out = vec![];
    for p in parts {
        let Source::Text(t) = &p.src else {
            out.push(p.clone());
            continue;
        };
        let (Place::Fit(x0, y0, x1, y1) | Place::Stretch(x0, y0, x1, y1)) = p.place else {
            return Err(format!("字の並び \"{t}\" には置く枠 (~ か >) が要ります").into());
        };
        let mut inks = vec![];
        for ch in t.chars() {
            let shape = source(&Source::Char(ch as u32), base)?;
            inks.push((ch, bbox(&round::flatten(&shape, FLAT_TOL))));
        }
        let width = |b: &Option<(f64, f64, f64, f64)>| b.map_or(4.0 * TEXT_GAP, |b| b.2 - b.0);
        let total = inks.iter().map(|(_, b)| width(b)).sum::<f64>() + TEXT_GAP * (inks.len().max(1) - 1) as f64;
        let band = TEXT_BAND.1 - TEXT_BAND.0;
        let s = ((x1 - x0) / total).min((y1 - y0) / band);
        let mut x = (x0 + x1) / 2.0 - total * s / 2.0;
        let yb = (y0 + y1) / 2.0 - band * s / 2.0;
        for (ch, b) in &inks {
            if let Some(b) = b {
                out.push(Part {
                    sub: p.sub,
                    src: Source::Char(*ch as u32),
                    pick: None,
                    crop: Some(vec![(b.0, TEXT_BAND.0), (b.2, TEXT_BAND.0), (b.2, TEXT_BAND.1), (b.0, TEXT_BAND.1)]),
                    place: Place::Stretch(x, yb, x + (b.2 - b.0) * s, yb + band * s),
                    weight: p.weight,
                });
            }
            x += (width(b) + TEXT_GAP) * s;
        }
    }
    Ok(out)
}

/// 部品を置いて (拡縮・切り抜き) 格子に塗る
pub fn compose(parts: &[Part], base: &Base) -> Result<(Grid, Vec<bool>)> {
    let parts = expand(parts, base)?;
    let mut placed = vec![];
    let mut all: Option<(f64, f64, f64, f64)> = None;
    let mut margin: f64 = 0.0;
    for p in &parts {
        let mut shape = source(&p.src, base)?;
        if let Some(pick) = &p.pick {
            let all = std::mem::take(&mut shape);
            for &i in pick {
                shape.push(all.get(i).ok_or_else(|| format!("{:?} に輪郭 {i} がありません", p.src))?.clone());
            }
        }
        let polys = round::flatten(&shape, FLAT_TOL);
        let ink = bbox(&polys).ok_or_else(|| format!("部品に形がありません: {:?}", p.src))?;
        let r = match &p.crop {
            Some(c) => bbox(std::slice::from_ref(c)).unwrap(),
            None => ink,
        };
        let (rw, rh) = ((r.2 - r.0).max(1e-9), (r.3 - r.1).max(1e-9));
        let (sx, sy, tx, ty) = match p.place {
            Place::Keep => (1.0, 1.0, 0.0, 0.0),
            Place::Stretch(x0, y0, x1, y1) => {
                let (sx, sy) = ((x1 - x0) / rw, (y1 - y0) / rh);
                (sx, sy, x0 - sx * r.0, y0 - sy * r.1)
            }
            Place::Fit(x0, y0, x1, y1) => {
                let s = ((x1 - x0) / rw).min((y1 - y0) / rh);
                (s, s, (x0 + x1) / 2.0 - s * (r.0 + r.2) / 2.0, (y0 + y1) / 2.0 - s * (r.1 + r.3) / 2.0)
            }
            Place::Shift(dx, dy) => (1.0, 1.0, dx, dy),
            Place::CenterX(x) => (1.0, 1.0, x - (r.0 + r.2) / 2.0, 0.0),
        };
        let tf = |q: &(f64, f64)| (q.0 * sx + tx, q.1 * sy + ty);
        let polys: Vec<Poly> = polys.iter().map(|c| c.iter().map(tf).collect()).collect();
        let crop: Option<Poly> = p.crop.as_ref().map(|c| c.iter().map(tf).collect());
        // 縮めた (伸ばした) 線を w の太さに戻す。図形 (輪・枠) は置いた大きさで描くので戻さない
        let comp = |s: f64| if p.weight > 0.0 && matches!(p.src, Source::Char(_)) && (s - 1.0).abs() > 1e-6 { (p.weight - STEM * s) / 2.0 } else { 0.0 };
        let grow = (comp(sx.abs()), comp(sy.abs()));
        let b = match &crop {
            Some(c) => bbox(std::slice::from_ref(c)).unwrap(),
            None => bbox(&polys).unwrap(),
        };
        margin = margin.max(grow.0.abs()).max(grow.1.abs());
        all = Some(union(all, b));
        placed.push(Placed { sub: p.sub, polys, crop, grow });
    }
    let all = all.ok_or("部品がありません")?;
    let g = Grid::covering(all, round::R1 + margin + 4.0);
    let mut mask = vec![false; g.w * g.h];
    for p in &placed {
        let mut m = g.fill(&p.polys);
        if let Some(c) = &p.crop {
            let cm = g.fill(std::slice::from_ref(c));
            for (v, k) in m.iter_mut().zip(cm) {
                *v &= k;
            }
        }
        round::grow(&mut m, g.w, g.h, 0, p.grow.0 / round::RES);
        round::grow(&mut m, g.w, g.h, 1, p.grow.1 / round::RES);
        for (v, k) in mask.iter_mut().zip(m) {
            if p.sub {
                *v &= !k;
            } else {
                *v |= k;
            }
        }
    }
    Ok((g, mask))
}

/// 楽器の略記: 半角の字を横に縮めて並べた部品 (1 マスに 4 字まで入る幅。縦は字面の中心を保って縮める)
pub fn compose_text(text: &str) -> Vec<Part> {
    const SX: f64 = 0.5;
    const SY: f64 = 0.8;
    let n = text.chars().count() as f64;
    let width = 512.0 * SX * n;
    let mut x = if text.starts_with('(') && !text.ends_with(')') {
        1024.0 - width // 左半分: 右に寄せて次のマスへ続ける
    } else if text.ends_with(')') && !text.starts_with('(') {
        0.0
    } else {
        (1024.0 - width) / 2.0
    };
    let mut out = vec![];
    for ch in text.chars() {
        // 半角の送り幅の枠 (0〜512) を縮めて並べる。縦は 280 を中心に縮める
        let (y0, y1) = (-200.0, 900.0);
        out.push(Part {
            sub: false,
            src: Source::Char(ch as u32),
            pick: None,
            crop: Some(vec![(0.0, y0), (512.0, y0), (512.0, y1), (0.0, y1)]),
            place: Place::Stretch(x, 280.0 + (y0 - 280.0) * SY, x + 512.0 * SX, 280.0 + (y1 - 280.0) * SY),
            weight: 50.0,
        });
        x += 512.0 * SX;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfnt::Pt;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
        vec![[(x0, y0), (x0, y1), (x1, y1), (x1, y0)].iter().map(|&(x, y)| Pt { x, y, on: true }).collect()]
    }

    #[test]
    fn text_is_packed_into_box() {
        let mut base = Base::new();
        base.insert('5' as u32, (rect(60.0, 0.0, 450.0, 800.0), 512));
        base.insert('.' as u32, (rect(200.0, 0.0, 300.0, 100.0), 512));
        base.insert('1' as u32, (rect(150.0, 0.0, 350.0, 800.0), 512));
        let p = crate::parts::load_str("U+1F1A0 x = \"5.1\" ~ 100,100,900,600 w60").unwrap();
        let crate::parts::Entry::Parts { parts, .. } = &p[&0x1F1A0] else { panic!() };
        let out = expand(parts, &base).unwrap();
        assert_eq!(out.len(), 3);
        let boxes: Vec<(f64, f64, f64, f64)> = out.iter().map(|q| match q.place { Place::Stretch(a, b, c, d) => (a, b, c, d), _ => panic!() }).collect();
        // 枠に収まり、左から順に重ならずに並び、横の真ん中は枠の真ん中
        for b in &boxes {
            assert!(b.0 >= 100.0 - 1e-6 && b.2 <= 900.0 + 1e-6 && b.1 >= 100.0 - 1e-6 && b.3 <= 600.0 + 1e-6, "{b:?}");
        }
        assert!(boxes.windows(2).all(|w| w[0].2 < w[1].0));
        let mid = (boxes[0].0 + boxes[2].2) / 2.0;
        assert!((mid - 500.0).abs() < 1e-6, "{mid}");
        assert!(out.iter().all(|q| q.weight == 60.0 && matches!(q.src, Source::Char(_))));
    }
}

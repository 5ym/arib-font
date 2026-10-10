//! 源柔ゴシック等幅に無い字を、源柔の字の輪郭と、源柔の線の太さ・丸みに合わせた図形で描く。
//! 描き方の数値はこのファイルの頭 (定数) にまとめる。座標は em 1024。
//!
//! 図形の向き: TrueType は塗る輪郭が時計回り、抜く輪郭が反時計回り。重なった塗りは足し合わさるので
//! (非ゼロ規則)、白抜きは塗りの内側にだけ置く。

use crate::sfnt::{Contour, Glyphs, Pt};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::f64::consts::PI;

type Cmap = BTreeMap<u32, u32>;
type Shape = Vec<Contour>;

// ---------------------------------------------------------------- 数値 (源柔から測ったもの)

/// 字面の中心 (□ ○ の中心)
const CX: f64 = 512.0;
const CY: f64 = 389.0;
/// □ の外枠 (102〜922, -20〜799) と線の太さ
const BOX: (f64, f64, f64, f64) = (102.0, -20.0, 922.0, 799.0);
const BOX_W: f64 = 38.0;
/// ○ の外の半径と線の太さ (51〜973)
const CIRCLE_R: f64 = 461.0;
const CIRCLE_W: f64 = 38.0;
/// 絵の線 (☁ ☂ の線に合わせる)
const LINE: f64 = 44.0;
/// 太い線 (⭕ などの heavy)
const HEAVY: f64 = 92.0;
/// 角の丸み
const ROUND: f64 = 36.0;
/// 楽器の略記: 半角の字を横・縦にこの比で縮めて並べる
const COMPOSE_SX: f64 = 0.5;
const COMPOSE_SY: f64 = 0.8;

// ---------------------------------------------------------------- 図形

fn area(c: &[(f64, f64)]) -> f64 {
    let n = c.len();
    (0..n).map(|i| c[i].0 * c[(i + 1) % n].1 - c[(i + 1) % n].0 * c[i].1).sum::<f64>() / 2.0
}

fn on(pts: &[(f64, f64)]) -> Contour {
    pts.iter().map(|&(x, y)| Pt { x, y, on: true }).collect()
}

/// 多角形を塗る (時計回りにそろえる)
fn poly(pts: &[(f64, f64)]) -> Shape {
    let mut v = pts.to_vec();
    if area(&v) > 0.0 {
        v.reverse();
    }
    vec![on(&v)]
}

/// 多角形を抜く (反時計回り)
fn poly_hole(pts: &[(f64, f64)]) -> Shape {
    let mut v = pts.to_vec();
    if area(&v) < 0.0 {
        v.reverse();
    }
    vec![on(&v)]
}

fn reverse(s: Shape) -> Shape {
    s.into_iter().map(|mut c| {
        c.reverse();
        c
    }).collect()
}

/// 楕円 (塗り)。8 つの off-curve の点で描く (間の on-curve は暗黙)
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Shape {
    let k = 1.0 / (PI / 8.0).cos();
    vec![(0..8)
        .map(|i| {
            let a = PI / 8.0 - i as f64 * PI / 4.0; // 時計回り
            Pt { x: cx + rx * k * a.cos(), y: cy + ry * k * a.sin(), on: false }
        })
        .collect()]
}

fn circle(cx: f64, cy: f64, r: f64) -> Shape {
    ellipse(cx, cy, r, r)
}

/// 輪 (外の半径と太さ)
fn ring(cx: f64, cy: f64, r: f64, w: f64) -> Shape {
    let mut s = circle(cx, cy, r);
    s.extend(reverse(circle(cx, cy, r - w)));
    s
}

fn ellipse_ring(cx: f64, cy: f64, rx: f64, ry: f64, w: f64) -> Shape {
    let mut s = ellipse(cx, cy, rx, ry);
    s.extend(reverse(ellipse(cx, cy, rx - w, ry - w)));
    s
}

/// 角を丸めた四角 (塗り)
fn rrect(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> Shape {
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    if r <= 0.5 {
        return poly(&[(x0, y1), (x1, y1), (x1, y0), (x0, y0)]);
    }
    let p = |x, y, on| Pt { x, y, on };
    vec![vec![
        p(x0 + r, y1, true),
        p(x1 - r, y1, true),
        p(x1, y1, false),
        p(x1, y1 - r, true),
        p(x1, y0 + r, true),
        p(x1, y0, false),
        p(x1 - r, y0, true),
        p(x0 + r, y0, true),
        p(x0, y0, false),
        p(x0, y0 + r, true),
        p(x0, y1 - r, true),
        p(x0, y1, false),
    ]]
}

/// 角丸の枠 (外形と太さ)
fn frame(x0: f64, y0: f64, x1: f64, y1: f64, r: f64, w: f64) -> Shape {
    let mut s = rrect(x0, y0, x1, y1, r);
    s.extend(reverse(rrect(x0 + w, y0 + w, x1 - w, y1 - w, (r - w).max(0.0))));
    s
}

/// 端の丸い線 (太さ w)
fn capsule(a: (f64, f64), b: (f64, f64), w: f64) -> Shape {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let (ux, uy) = (dx / len, dy / len);
    let h = w / 2.0;
    let (nx, ny) = (-uy * h, ux * h);
    let (tx, ty) = (ux * h, uy * h);
    let p = |x, y, on| Pt { x, y, on };
    let c = vec![
        p(a.0 + nx, a.1 + ny, true),
        p(b.0 + nx, b.1 + ny, true),
        p(b.0 + nx + tx, b.1 + ny + ty, false),
        p(b.0 + tx, b.1 + ty, true),
        p(b.0 - nx + tx, b.1 - ny + ty, false),
        p(b.0 - nx, b.1 - ny, true),
        p(a.0 - nx, a.1 - ny, true),
        p(a.0 - nx - tx, a.1 - ny - ty, false),
        p(a.0 - tx, a.1 - ty, true),
        p(a.0 + nx - tx, a.1 + ny - ty, false),
    ];
    let pts: Vec<(f64, f64)> = c.iter().map(|q| (q.x, q.y)).collect();
    if area(&pts) > 0.0 {
        reverse(vec![c])
    } else {
        vec![c]
    }
}

/// 折れ線 (端と角は丸い)
fn stroke(pts: &[(f64, f64)], w: f64) -> Shape {
    pts.windows(2).flat_map(|p| capsule(p[0], p[1], w)).collect()
}

/// 円弧の線 (角度は度、a0 から a1 へ)
fn arc(cx: f64, cy: f64, r: f64, a0: f64, a1: f64, w: f64) -> Shape {
    let n = (((a1 - a0).abs() / 10.0).ceil() as usize).max(2);
    let pt = |rr: f64, a: f64| (cx + rr * a.to_radians().cos(), cy + rr * a.to_radians().sin());
    let mut v: Vec<(f64, f64)> = (0..=n).map(|i| pt(r + w / 2.0, a0 + (a1 - a0) * i as f64 / n as f64)).collect();
    v.extend((0..=n).rev().map(|i| pt(r - w / 2.0, a0 + (a1 - a0) * i as f64 / n as f64)));
    let mut s = poly(&v);
    s.extend(circle(pt(r, a0).0, pt(r, a0).1, w / 2.0));
    s.extend(circle(pt(r, a1).0, pt(r, a1).1, w / 2.0));
    s
}

fn map(s: Shape, f: impl Fn(f64, f64) -> (f64, f64)) -> Shape {
    s.into_iter()
        .map(|c| {
            c.into_iter()
                .map(|q| {
                    let (x, y) = f(q.x, q.y);
                    Pt { x, y, on: q.on }
                })
                .collect()
        })
        .collect()
}

/// 拡縮して動かす (中心 (cx, cy) を (tx, ty) へ)
fn place(s: Shape, cx: f64, cy: f64, sx: f64, sy: f64, tx: f64, ty: f64) -> Shape {
    let s = map(s, |x, y| ((x - cx) * sx + tx, (y - cy) * sy + ty));
    if sx * sy < 0.0 { reverse(s) } else { s }
}

/// 回す (度、中心まわり)
fn rotate(s: Shape, cx: f64, cy: f64, deg: f64) -> Shape {
    let (sn, cs) = deg.to_radians().sin_cos();
    map(s, |x, y| (cx + (x - cx) * cs - (y - cy) * sn, cy + (x - cx) * sn + (y - cy) * cs))
}

/// 左右を返す
fn mirror(s: Shape) -> Shape {
    reverse(map(s, |x, y| (1024.0 - x, y)))
}

fn bbox(s: &Shape) -> (f64, f64, f64, f64) {
    let pts = s.iter().flatten();
    let (mut a, mut b, mut c, mut d) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in pts {
        (a, b, c, d) = (a.min(q.x), b.min(q.y), c.max(q.x), d.max(q.y));
    }
    (a, b, c, d)
}

struct Base<'a> {
    glyphs: &'a Glyphs,
    cmap: &'a Cmap,
}

impl Base<'_> {
    /// 源柔の字の輪郭
    fn glyph(&self, ch: char) -> Result<Shape> {
        let gid = *self.cmap.get(&(ch as u32)).with_context(|| format!("源柔に {ch} がありません"))?;
        self.glyphs.outline(gid as usize)
    }
    /// 源柔の字の、x が split より左 (left) か右の輪郭だけ
    fn half(&self, ch: char, split: f64, left: bool) -> Result<Shape> {
        Ok(self
            .glyph(ch)?
            .into_iter()
            .filter(|c| {
                let xs = c.iter().map(|q| q.x);
                if left { xs.fold(f64::MIN, f64::max) <= split } else { xs.fold(f64::MAX, f64::min) >= split }
            })
            .collect())
    }
}

// ---------------------------------------------------------------- 字ごと

/// 描く字。無ければ None
pub fn draw(c: u32, glyphs: &Glyphs, cmap: &Cmap) -> Result<Option<Shape>> {
    let b = Base { glyphs, cmap };
    let (bx0, by0, bx1, by1) = BOX;
    let mut s: Shape = vec![];
    match c {
        // ---- 源柔の字をそのまま・回して使う
        0x0FD6 => s = b.glyph('卍')?, // ࿖ 左向きの卍 (地図の寺)
        0x26C4 => s = b.glyph('☃')?, // ⛄ 雪だるま (雪なし)
        0x26C9 => s = rotate(b.glyph('☖')?, CX, CY, 180.0), // ⛉ 白い将棋の駒を逆さに
        0x26CA => s = rotate(b.glyph('☗')?, CX, CY, 180.0), // ⛊ 黒い将棋の駒を逆さに

        // ---- 分数 (源柔の ⅓ の数字の置き場に、半角の数字を同じ高さで置く)
        0x2150 => s = fraction(&b, None, "7")?,
        0x2151 => s = fraction(&b, None, "9")?,
        0x2152 => s = fraction(&b, None, "10")?,
        0x2189 => s = fraction(&b, Some("0"), "3")?,

        // ---- 丸・四角・楕円
        0x2B1B => s = b.glyph('■')?.into_iter().collect(), // ⬛ (源柔の ■ と同じ大きさ)
        0x2B24 => s = circle(CX, CY, CIRCLE_R),             // ⬤
        0x2B2E => s = ellipse(CX, CY, 230.0, CIRCLE_R),      // ⬮ 縦長の黒い楕円
        0x2B2F => s = ellipse_ring(CX, CY, 230.0, CIRCLE_R, CIRCLE_W), // ⬯
        0x2B55 | 0x2B58 => s = ring(CX, CY, CIRCLE_R, HEAVY),            // ⭕ ⭘ 太い輪
        0x2B57 => {
            // ⭗ 太い輪の中に輪
            s = ring(CX, CY, CIRCLE_R, HEAVY);
            s.extend(ring(CX, CY, CIRCLE_R - HEAVY - 70.0, CIRCLE_W));
        }
        0x2B56 => {
            // ⭖ 太い楕円の中に楕円 (横長)
            s = ellipse_ring(CX, CY, CIRCLE_R, 260.0, HEAVY);
            s.extend(ellipse_ring(CX, CY, CIRCLE_R - HEAVY - 60.0, 260.0 - HEAVY - 60.0, CIRCLE_W));
        }
        0x2B59 => {
            // ⭙ 太い輪にばつ
            s = ring(CX, CY, CIRCLE_R, HEAVY);
            let d = (CIRCLE_R - HEAVY / 2.0) * 0.70;
            s.extend(capsule((CX - d, CY - d), (CX + d, CY + d), HEAVY * 0.7));
            s.extend(capsule((CX - d, CY + d), (CX + d, CY - d), HEAVY * 0.7));
        }
        0x2A00 => {
            // ⨀ 丸に点
            s = ring(CX, CY, CIRCLE_R, HEAVY * 0.7);
            s.extend(circle(CX, CY, 120.0));
        }
        0x27D0 => {
            // ⟐ ひし形に点 (源柔の ◇ に)
            s = b.glyph('◇')?;
            s.extend(circle(CX, CY, 90.0));
        }
        0x233A | 0x26CB => s = quad_diamond(&b)?, // ⌺ ⛋ □ にひし形
        0x26DD => {
            // ⛝ □ にばつ
            s = frame(bx0, by0, bx1, by1, 0.0, BOX_W * 2.0);
            s.extend(stroke(&[(bx0 + 40.0, by0 + 40.0), (bx1 - 40.0, by1 - 40.0)], LINE * 1.6));
            s.extend(stroke(&[(bx0 + 40.0, by1 - 40.0), (bx1 - 40.0, by0 + 40.0)], LINE * 1.6));
        }
        0x26DE => {
            // ⛞ 黒い四角の中の白い丸に斜めの線
            s = rrect(bx0, by0, bx1, by1, 0.0);
            s.extend(reverse(circle(CX, CY, 370.0)));
            let d = 370.0 * 0.72;
            s.extend(capsule((CX - d, CY + d), (CX + d, CY - d), HEAVY * 1.3));
        }
        0x26F6 => {
            // ⛶ 四隅のかぎ
            let (l, w) = (260.0, HEAVY);
            for (x, y, sx, sy) in [(bx0, by1, 1.0, -1.0), (bx1, by1, -1.0, -1.0), (bx0, by0, 1.0, 1.0), (bx1, by0, -1.0, 1.0)] {
                s.extend(poly(&[(x, y), (x + sx * l, y), (x + sx * l, y + sy * w), (x + sx * w, y + sy * w), (x + sx * w, y + sy * l), (x, y + sy * l)]));
            }
        }
        0x26DB => {
            // ⛛ 太い白い下向きの三角
            let t = [(bx0, by1), (bx1, by1), (CX, by0)];
            s = stroke(&[t[0], t[1], t[2], t[0]], HEAVY);
        }
        0x2613 => {
            // ☓ ばつ
            let d = 300.0;
            s = capsule((CX - d, CY - d), (CX + d, CY + d), LINE * 1.2);
            s.extend(capsule((CX - d, CY + d), (CX + d, CY - d), LINE * 1.2));
        }
        0x2757 => {
            // ❗ 太い感嘆符
            s = poly(&[(CX - 85.0, 800.0), (CX + 85.0, 800.0), (CX + 50.0, 200.0), (CX - 50.0, 200.0)]);
            s.extend(rrect(CX - 85.0, 640.0, CX + 85.0, 820.0, 60.0));
            s.extend(circle(CX, 50.0, 85.0));
        }
        0x2985 | 0x2986 => {
            // ⦅ ⦆ 二重の丸括弧 (源柔の （ を2つ重ねる)
            let p = b.glyph('（')?;
            let (x0, _, x1, _) = bbox(&p);
            let w = x1 - x0;
            s = place(p.clone(), 0.0, 0.0, 1.0, 1.0, 0.0, 0.0);
            s.extend(place(p, 0.0, 0.0, 1.0, 1.0, w * 0.55, 0.0));
            if c == 0x2986 {
                s = mirror(s);
            }
        }

        // ---- 天気
        0x26A1 => s = bolt(CX, CY, 1.0), // ⚡
        0x2614 => {
            // ☔ 傘に雨
            s = place(b.glyph('☂')?, CX, -61.0, 0.8, 0.8, CX + 40.0, -61.0);
            for (x, y) in [(200.0, 760.0), (420.0, 820.0), (700.0, 790.0), (880.0, 700.0)] {
                s.extend(capsule((x, y), (x - 30.0, y - 80.0), LINE));
            }
        }
        0x26C5 => {
            // ⛅ 雲の後ろの太陽: 源柔の ☀ と、☁ の外形を塗ったもの
            s = place(b.glyph('☀')?, CX, CY, 0.62, 0.62, 650.0, 560.0);
            s.extend(place(cloud(&b)?, CX, CY, 0.8, 0.8, 440.0, 260.0));
        }
        0x26C6 => s = rain(CX, CY, 1.0), // ⛆ 雨
        0x26C7 => {
            // ⛇ 黒い雪だるま
            s = circle(CX, 150.0, 240.0);
            s.extend(circle(CX, 540.0, 170.0));
            s.extend(rrect(CX - 140.0, 660.0, CX + 140.0, 840.0, 30.0));
        }
        0x26C8 => {
            // ⛈ 雷雲と雨
            s = place(cloud(&b)?, CX, CY, 0.85, 0.7, CX, 560.0);
            s.extend(bolt(560.0, 170.0, 0.5));
            for (x, y) in [(220.0, 260.0), (300.0, 80.0), (760.0, 260.0), (840.0, 80.0)] {
                s.extend(capsule((x, y), (x - 25.0, y - 80.0), LINE));
            }
        }

        // ---- 道路・交通
        0x26CC => {
            // ⛌ 交差する車線
            s = capsule((180.0, 60.0), (844.0, 720.0), HEAVY);
            s.extend(capsule((180.0, 720.0), (844.0, 60.0), HEAVY));
        }
        0x26D2 => {
            // ⛒ 丸の中の交差する車線
            s = ring(CX, CY, CIRCLE_R, LINE * 1.5);
            let d = 260.0;
            s.extend(capsule((CX - d, CY - d), (CX + d, CY + d), HEAVY * 0.8));
            s.extend(capsule((CX - d, CY + d), (CX + d, CY - d), HEAVY * 0.8));
        }
        0x26D4 => {
            // ⛔ 進入禁止
            s = circle(CX, CY, CIRCLE_R);
            s.extend(reverse(rrect(CX - 320.0, CY - 75.0, CX + 320.0, CY + 75.0, 30.0)));
        }
        0x26CD => {
            // ⛍ 故障した車 (三角の上の車)
            s = place(car(), CX, 400.0, 0.62, 0.62, CX, 560.0);
            let t = [(150.0, -20.0), (874.0, -20.0), (CX, 420.0)];
            s.extend(stroke(&[t[0], t[1], t[2], t[0]], LINE * 1.4));
        }
        0x26D0 => {
            // ⛐ 滑る車
            s = rotate(place(car(), CX, 400.0, 0.7, 0.7, 600.0, 470.0), 600.0, 470.0, 20.0);
            s.extend(stroke(&[(120.0, 120.0), (230.0, 230.0), (130.0, 340.0), (240.0, 450.0)], LINE));
        }
        0x26DF => {
            // ⛟ 黒いトラック
            s = rrect(110.0, 210.0, 640.0, 600.0, 20.0);
            s.extend(poly(&[(660.0, 210.0), (914.0, 210.0), (914.0, 420.0), (820.0, 520.0), (660.0, 520.0)]));
            for x in [270.0, 790.0] {
                s.extend(circle(x, 180.0, 85.0));
            }
        }
        0x26D5 => s = road_sign(1),  // ⛕ 左側通行の一方通行 (互い違い)
        0x26D6 => s = diamond_arrows(true),  // ⛖ 黒いひし形に上下の矢印
        0x26D7 => s = diamond_arrows(false), // ⛗ 白いひし形に上下の矢印
        0x26D8 => s = road_sign(2),  // ⛘ 黒い左の車線の合流
        0x26D9 => s = road_sign(3),  // ⛙ 白い左の車線の合流
        0x26DA => s = road_sign(4),  // ⛚ 徐行
        0x26DC => s = road_sign(5),  // ⛜ 左の入口が閉じている
        0x26E0 => s = road_sign(6),  // ⛠ 左の入口の制限 1
        0x26E1 => s = road_sign(7),  // ⛡ 左の入口の制限 2
        0x26E3 => {
            // ⛣ 太い輪に縦の線と上に2つの点 (ö の形)
            s = ring(CX, 300.0, 330.0, HEAVY * 0.8);
            s.extend(capsule((CX, 560.0), (CX, 690.0), LINE * 1.2));
            for x in [CX - 190.0, CX + 190.0] {
                s.extend(circle(x, 760.0, 70.0));
            }
        }

        // ---- 地図・施設
        0x26E8 => {
            // ⛨ 盾に十字
            let sh = [(bx0, by1), (bx1, by1), (bx1, 260.0), (CX, by0), (bx0, 260.0)];
            s = stroke(&[sh[0], sh[1], sh[2], sh[3], sh[4], sh[0]], LINE * 1.3);
            s.extend(capsule((CX, 230.0), (CX, 640.0), LINE * 1.6));
            s.extend(capsule((CX - 200.0, 440.0), (CX + 200.0, 440.0), LINE * 1.6));
        }
        0x26E9 => {
            // ⛩ 鳥居
            s = poly(&[(80.0, 760.0), (944.0, 760.0), (900.0, 660.0), (124.0, 660.0)]);
            s.extend(rrect(170.0, 520.0, 854.0, 590.0, 10.0));
            for x in [300.0, 724.0] {
                s.extend(rrect(x - 40.0, -40.0, x + 40.0, 680.0, 10.0));
            }
        }
        0x26EA => {
            // ⛪ 教会
            s = poly(&[(150.0, -40.0), (874.0, -40.0), (874.0, 300.0), (CX, 560.0), (150.0, 300.0)]);
            s.extend(reverse(rrect(CX - 80.0, -40.0 + 0.1, CX + 80.0, 200.0, 70.0)));
            s.extend(capsule((CX, 560.0), (CX, 860.0), LINE * 1.3));
            s.extend(capsule((CX - 90.0, 760.0), (CX + 90.0, 760.0), LINE * 1.3));
        }
        0x26EB => {
            // ⛫ 城 (凸の形の線)
            s = stroke(
                &[(90.0, -20.0), (90.0, 420.0), (300.0, 420.0), (300.0, 640.0), (724.0, 640.0), (724.0, 420.0), (934.0, 420.0), (934.0, -20.0)],
                LINE * 1.3,
            );
        }
        0x26EC => {
            // ⛬ 史跡 (三角に並んだ3つの点)
            for (x, y) in [(CX, 560.0), (260.0, 120.0), (764.0, 120.0)] {
                s.extend(circle(x, y, 85.0));
            }
        }
        0x26ED | 0x26EE | 0x26EF => {
            // ⛭ 軸の無い歯車 / ⛮ 取っ手のある歯車 / ⛯ 灯台
            let r = 230.0;
            s = ring(CX, CY, r, LINE * 1.2);
            if c == 0x26EF {
                s.extend(circle(CX, CY, 100.0));
            }
            let n = if c == 0x26EE { 4 } else { 8 };
            for i in 0..n {
                let a = (i as f64) * 360.0 / n as f64;
                let (sn, cs) = a.to_radians().sin_cos();
                let (r0, r1) = (r + 40.0, if c == 0x26EE { 450.0 } else { 400.0 });
                s.extend(capsule((CX + cs * r0, CY + sn * r0), (CX + cs * r1, CY + sn * r1), LINE * 1.2));
            }
        }
        0x26F0 => {
            // ⛰ 山
            s = poly(&[(80.0, -20.0), (944.0, -20.0), (600.0, 760.0), (424.0, 760.0)]);
            s.extend(circle(CX, 700.0, 110.0));
        }
        0x26F1 => {
            // ⛱ 地面に立てた傘
            s = poly(&(0..=18).map(|i| {
                let a = (i as f64 * 10.0).to_radians();
                (CX + 400.0 * a.cos(), 400.0 + 360.0 * a.sin())
            }).collect::<Vec<_>>());
            s.extend(capsule((CX, 380.0), (CX - 120.0, -20.0), LINE));
            s.extend(capsule((80.0, -20.0), (944.0, -20.0), LINE));
        }
        0x26F2 => {
            // ⛲ 噴水
            s = rrect(150.0, -20.0, 874.0, 180.0, 30.0);
            s.extend(rrect(CX - 60.0, 180.0, CX + 60.0, 420.0, 10.0));
            s.extend(arc(CX - 200.0, 420.0, 200.0, 0.0, 180.0, LINE));
            s.extend(arc(CX + 200.0, 420.0, 200.0, 0.0, 180.0, LINE));
        }
        0x26F3 => {
            // ⛳ 穴に立てた旗
            s = ellipse(CX, 60.0, 400.0, 90.0);
            s.extend(capsule((CX - 60.0, 60.0), (CX - 60.0, 840.0), LINE));
            s.extend(poly(&[(CX - 60.0, 840.0), (CX + 300.0, 720.0), (CX - 60.0, 600.0)]));
        }
        0x26F4 => {
            // ⛴ フェリー
            s = poly(&[(60.0, 260.0), (964.0, 260.0), (840.0, 20.0), (184.0, 20.0)]);
            s.extend(rrect(220.0, 260.0, 804.0, 480.0, 20.0));
            s.extend(rrect(400.0, 480.0, 620.0, 640.0, 10.0));
            for x in [320.0, 450.0, 580.0, 710.0] {
                s.extend(reverse(rrect(x - 35.0, 320.0, x + 35.0, 420.0, 10.0)));
            }
        }
        0x26F5 => {
            // ⛵ 帆船
            s = poly(&[(100.0, 180.0), (924.0, 180.0), (800.0, -20.0), (224.0, -20.0)]);
            s.extend(poly(&[(540.0, 840.0), (540.0, 240.0), (880.0, 240.0)]));
            s.extend(poly(&[(480.0, 700.0), (480.0, 240.0), (180.0, 240.0)]));
        }
        0x26F7 => {
            // ⛷ スキー
            s = circle(650.0, 700.0, 85.0);
            s.extend(stroke(&[(600.0, 560.0), (440.0, 360.0), (560.0, 220.0)], LINE * 1.8));
            s.extend(stroke(&[(440.0, 360.0), (300.0, 260.0)], LINE * 1.8));
            s.extend(capsule((120.0, 300.0), (900.0, -20.0), LINE));
        }
        0x26F8 => {
            // ⛸ スケート靴
            s = poly(&[(300.0, 820.0), (520.0, 820.0), (520.0, 400.0), (820.0, 300.0), (820.0, 180.0), (300.0, 180.0)]);
            s.extend(capsule((200.0, 40.0), (860.0, 40.0), LINE * 1.3));
            s.extend(capsule((360.0, 180.0), (360.0, 40.0), LINE));
            s.extend(capsule((760.0, 180.0), (760.0, 40.0), LINE));
        }
        0x26F9 => {
            // ⛹ 球を持つ人
            s = circle(400.0, 700.0, 95.0);
            s.extend(rrect(300.0, 220.0, 500.0, 580.0, 60.0));
            s.extend(capsule((340.0, 230.0), (300.0, -20.0), LINE * 1.8));
            s.extend(capsule((460.0, 230.0), (520.0, -20.0), LINE * 1.8));
            s.extend(capsule((480.0, 500.0), (700.0, 360.0), LINE * 1.5));
            s.extend(circle(790.0, 300.0, 120.0));
        }
        0x26FA => {
            // ⛺ テント
            s = poly(&[(80.0, -20.0), (944.0, -20.0), (CX, 740.0)]);
            s.extend(poly_hole(&[(CX - 140.0, -20.0 + 0.1), (CX + 140.0, -20.0 + 0.1), (CX, 300.0)]));
            s.extend(capsule((CX, 700.0), (CX, 840.0), LINE));
        }
        0x26FB => {
            // ⛻ 銀行 (縦長の輪の左右に切れ込み)
            s = ellipse_ring(CX, CY, 330.0, CIRCLE_R, LINE * 1.3);
            s.extend(capsule((CX - 330.0, CY), (CX - 180.0, CY), LINE * 1.3));
            s.extend(capsule((CX + 180.0, CY), (CX + 330.0, CY), LINE * 1.3));
        }
        0x26FC => {
            // ⛼ 墓地 (2つの墓石)
            for (x, h, w) in [(300.0, 760.0, 110.0), (720.0, 560.0, 100.0)] {
                s.extend(rrect(x - w, 60.0, x + w, h, 40.0));
                s.extend(rrect(x - w - 70.0, -20.0, x + w + 70.0, 80.0, 20.0));
            }
        }
        0x26FD => {
            // ⛽ 給油機
            s = rrect(150.0, -20.0, 600.0, 760.0, 60.0);
            s.extend(reverse(rrect(230.0, 440.0, 520.0, 680.0, 30.0)));
            s.extend(stroke(&[(600.0, 520.0), (780.0, 520.0), (780.0, 120.0), (880.0, 120.0), (880.0, 600.0), (800.0, 700.0)], LINE));
        }
        0x26FE => {
            // ⛾ 黒い四角に白い杯
            s = rrect(bx0, by0, bx1, by1, ROUND);
            s.extend(reverse(rrect(260.0, 220.0, 680.0, 600.0, 120.0)));
            s.extend(reverse(ring(790.0, 420.0, 100.0, LINE))); // 取っ手も白く抜く
            s.extend(reverse(rrect(220.0, 100.0, 760.0, 160.0, 20.0)));
        }
        0x26FF => {
            // ⛿ 白い旗に黒い横の帯
            s = frame(200.0, 300.0, 900.0, 800.0, 0.0, LINE);
            s.extend(rrect(200.0, 470.0, 900.0, 630.0, 0.0));
            s.extend(capsule((180.0, -20.0), (180.0, 800.0), LINE * 1.3));
        }
        0x2708 => {
            // ✈ 飛行機
            s = rrect(CX - 60.0, -20.0, CX + 60.0, 820.0, 60.0);
            s.extend(poly(&[(80.0, 420.0), (CX, 560.0), (944.0, 420.0), (944.0, 330.0), (CX, 420.0), (80.0, 330.0)]));
            s.extend(poly(&[(330.0, 70.0), (CX, 150.0), (694.0, 70.0), (694.0, 0.0), (CX, 60.0), (330.0, 0.0)]));
        }
        0x2693 => {
            // ⚓ 錨
            s = ring(CX, 720.0, 90.0, LINE);
            s.extend(capsule((CX, 630.0), (CX, 0.0), LINE * 1.3));
            s.extend(capsule((CX - 160.0, 520.0), (CX + 160.0, 520.0), LINE * 1.3));
            s.extend(arc(CX, 320.0, 340.0, 200.0, 340.0, LINE * 1.3));
        }
        0x269E | 0x269F => {
            // ⚞ 右に集まる3本の線 / ⚟ 左に集まる
            for y in [700.0, CY, 80.0] {
                s.extend(capsule((150.0, y), (880.0, CY), LINE * 1.3));
            }
            if c == 0x269F {
                s = mirror(s);
            }
        }
        0x26BF => {
            // ⚿ 四角の中の鍵
            s = frame(bx0, by0, bx1, by1, ROUND, BOX_W);
            s.extend(ring(CX, 560.0, 130.0, LINE * 1.2));
            s.extend(capsule((CX, 430.0), (CX, 80.0), LINE * 1.2));
            s.extend(capsule((CX, 140.0), (CX + 120.0, 140.0), LINE * 1.2));
            s.extend(capsule((CX, 240.0), (CX + 100.0, 240.0), LINE * 1.2));
        }
        0x26CF => {
            // ⛏ つるはし
            s = arc(CX, 300.0, 420.0, 40.0, 140.0, LINE * 2.0);
            s.extend(capsule((CX + 80.0, 690.0), (220.0, -20.0), LINE * 1.5));
        }
        0x26D1 => {
            // ⛑ 白い十字の兜
            s = poly(&(0..=18).map(|i| {
                let a = (i as f64 * 10.0).to_radians();
                (CX + 380.0 * a.cos(), 150.0 + 520.0 * a.sin())
            }).collect::<Vec<_>>());
            s.extend(rrect(60.0, 80.0, 964.0, 180.0, 40.0));
            s.extend(reverse(rrect(CX - 50.0, 280.0, CX + 50.0, 560.0, 10.0)));
            s.extend(reverse(rrect(CX - 140.0, 370.0, CX - 50.0, 470.0, 10.0)));
            s.extend(reverse(rrect(CX + 50.0, 370.0, CX + 140.0, 470.0, 10.0)));
        }
        0x26D3 => {
            // ⛓ 鎖
            // 両端は斜めの輪、真ん中は横から見た輪 (棒) で 2 つの輪の穴を通す
            let (a, z) = ((290.0, 640.0), (734.0, 138.0));
            for (x, y) in [a, z] {
                s.extend(rotate(ellipse_ring(x, y, 200.0, 115.0, LINE * 1.1), x, y, -48.0));
            }
            s.extend(capsule((a.0 + 70.0, a.1 - 80.0), (z.0 - 70.0, z.1 + 80.0), LINE * 1.6));
        }

        // ---- 漢字 (源柔の偏と旁を組む)
        0x55BC => s = kanji(&b, ('吡', 335.0), Right::Scale('急'))?, // 喼 ⿰口急
        0x40EF => s = kanji(&b, ('硎', 360.0), Right::Scale('楽'))?, // 䃯 ⿰石楽
        0x9FC5 => s = kanji(&b, ('祾', 380.0), Right::From('澪', 280.0))?, // 鿅 ⿰礻零
        _ => return Ok(None),
    }
    Ok(Some(s))
}

/// ⌺ (□ にひし形): 元の □ に、□ と同じ線の太さのひし形を内に描く (頂点は □ の線の真ん中)
fn quad_diamond(b: &Base) -> Result<Shape> {
    let mut out = b.glyph('□')?;
    let (x0, y0, x1, y1) = bbox(&out);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let ro = (x1 - x0) / 2.0 - BOX_W / 2.0;
    let rh = |v: f64| {
        // 前の作り方 (Python の round) と同じ点にする
        let r = v.round();
        if (v - v.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - v.signum() } else { r }
    };
    for (r, sign) in [(ro, 1.0), (ro - BOX_W * 2f64.sqrt(), -1.0)] {
        let pts = [(cx, cy + r), (cx + sign * r, cy), (cx, cy - r), (cx - sign * r, cy)];
        out.push(pts.iter().map(|&(x, y)| Pt { x: rh(x), y: rh(y), on: true }).collect());
    }
    Ok(out)
}

/// 分数: 源柔の ⅓ の分子・斜線・分母の置き場を測り、数字を同じ高さで置く
fn fraction(b: &Base, num: Option<&str>, den: &str) -> Result<Shape> {
    let third = b.glyph('⅓')?;
    let mut slash = vec![];
    let mut n1 = vec![];
    let mut d3 = vec![];
    for c in third {
        let (x0, y0, x1, _) = bbox(&vec![c.clone()]);
        if x1 - x0 > 400.0 {
            slash.push(c);
        } else if y0 > 200.0 {
            n1.push(c);
        } else {
            d3.push(c);
        }
    }
    let digits = |text: &str, slot: &Shape| -> Result<Shape> {
        let (sx0, sy0, sx1, sy1) = bbox(slot);
        let h = sy1 - sy0;
        let gap = h * 0.08;
        let mut parts = vec![];
        let mut width = 0.0;
        for ch in text.chars() {
            let g = b.glyph(ch)?;
            let (x0, y0, x1, y1) = bbox(&g);
            let k = h / (y1 - y0);
            let p = place(g, x0, y0, k, k, 0.0, 0.0);
            let w = (x1 - x0) * k;
            parts.push((p, width));
            width += w + gap;
        }
        width -= gap;
        let fit = ((sx1 - sx0) * 1.15 / width).min(1.0);
        let cx = (sx0 + sx1) / 2.0;
        let mut out = vec![];
        for (p, x) in parts {
            out.extend(place(p, 0.0, 0.0, fit, 1.0, cx - width * fit / 2.0 + x * fit, sy0));
        }
        Ok(out)
    };
    let mut out = slash;
    out.extend(match num {
        Some(t) => digits(t, &n1)?,
        None => n1,
    });
    out.extend(if den == "3" { d3 } else { digits(den, &d3)? });
    Ok(out)
}

/// ☁ の外形を塗ったもの (源柔の ☁ は線なので、外の輪郭だけ使う)
fn cloud(b: &Base) -> Result<Shape> {
    let g = b.glyph('☁')?;
    let big = g
        .into_iter()
        .max_by(|a, c| {
            let (a0, _, a1, _) = bbox(&vec![a.clone()]);
            let (c0, _, c1, _) = bbox(&vec![c.clone()]);
            (a1 - a0).partial_cmp(&(c1 - c0)).unwrap()
        })
        .context("☁ に輪郭がありません")?;
    Ok(vec![big])
}

/// 稲妻
fn bolt(cx: f64, cy: f64, k: f64) -> Shape {
    let p = [(-60.0, 460.0), (200.0, 460.0), (40.0, 60.0), (220.0, 60.0), (-160.0, -440.0), (-10.0, -60.0), (-200.0, -60.0)];
    poly(&p.iter().map(|&(x, y)| (cx + x * k, cy + y * k)).collect::<Vec<_>>())
}

/// 雨 (斜めの短い線を並べる)
fn rain(cx: f64, cy: f64, k: f64) -> Shape {
    let mut s = vec![];
    for row in 0..3 {
        for col in 0..4 {
            let x = cx + (col as f64 - 1.5) * 220.0 * k + if row % 2 == 1 { 110.0 * k } else { 0.0 };
            let y = cy + (1.0 - row as f64) * 280.0 * k;
            s.extend(capsule((x + 40.0 * k, y + 90.0 * k), (x - 40.0 * k, y - 90.0 * k), LINE));
        }
    }
    s
}

/// 車 (横から)
fn car() -> Shape {
    let mut s = rrect(120.0, 180.0, 904.0, 440.0, 70.0);
    s.extend(poly(&[(260.0, 440.0), (764.0, 440.0), (660.0, 640.0), (364.0, 640.0)]));
    for x in [300.0, 724.0] {
        s.extend(circle(x, 170.0, 100.0));
    }
    s
}

/// ひし形に上下の矢印 (⛖ 黒 / ⛗ 白)
fn diamond_arrows(black: bool) -> Shape {
    let d = [(CX, 830.0), (950.0, CY), (CX, -52.0), (74.0, CY)];
    let mut s;
    let arrows = |s: &mut Shape, hole: bool| {
        let shafts = [
            vec![(CX - 150.0, 160.0), (CX - 150.0, 470.0), (CX - 220.0, 470.0), (CX - 110.0, 640.0), (CX - 0.0, 470.0), (CX - 70.0, 470.0), (CX - 70.0, 160.0)],
            vec![(CX + 150.0, 620.0), (CX + 150.0, 310.0), (CX + 220.0, 310.0), (CX + 110.0, 140.0), (CX + 0.0, 310.0), (CX + 70.0, 310.0), (CX + 70.0, 620.0)],
        ];
        for a in shafts {
            s.extend(if hole { poly_hole(&a) } else { poly(&a) });
        }
    };
    if black {
        s = poly(&d);
        arrows(&mut s, true);
    } else {
        s = stroke(&[d[0], d[1], d[2], d[3], d[0]], LINE * 1.2);
        arrows(&mut s, false);
    }
    s
}

/// 道路の標識 (ARIB の 93区の道路の記号。車線の線と黒い塊で描く)
fn road_sign(kind: u8) -> Shape {
    let mut s = vec![];
    let dash = |s: &mut Shape, x: f64| {
        for y in [-20.0, 240.0, 500.0] {
            s.extend(rrect(x - 22.0, y, x + 22.0, y + 160.0, 10.0));
        }
    };
    match kind {
        1 => {
            // ⛕ 互い違いの一方通行: 黒い塊に上と下の矢印
            s = rrect(150.0, -20.0, 874.0, 800.0, ROUND);
            s.extend(poly_hole(&[(300.0, 100.0), (300.0, 500.0), (220.0, 500.0), (360.0, 700.0), (500.0, 500.0), (420.0, 500.0), (420.0, 100.0)]));
            s.extend(poly_hole(&[(724.0, 680.0), (724.0, 280.0), (804.0, 280.0), (664.0, 80.0), (524.0, 280.0), (604.0, 280.0), (604.0, 680.0)]));
        }
        2 | 3 => {
            // ⛘ ⛙ 左の車線の合流: 右の車線の線と、左から寄る線
            if kind == 2 {
                s = poly(&[(150.0, -20.0), (480.0, -20.0), (480.0, 800.0), (350.0, 800.0), (150.0, 500.0)]);
            } else {
                s = stroke(&[(150.0, -20.0), (150.0, 400.0), (380.0, 800.0)], LINE);
            }
            dash(&mut s, 640.0);
            s.extend(capsule((874.0, -20.0), (874.0, 800.0), LINE));
        }
        4 => {
            // ⛚ 徐行: 太いかぎを向かい合わせる
            for (x, sx) in [(150.0, 1.0), (874.0, -1.0)] {
                s.extend(poly(&[(x, 800.0), (x + sx * 260.0, 800.0), (x + sx * 260.0, 660.0), (x + sx * 120.0, 660.0), (x + sx * 120.0, 120.0), (x + sx * 260.0, 120.0), (x + sx * 260.0, -20.0), (x, -20.0)]));
            }
        }
        5 => {
            // ⛜ 左の入口が閉じている: 車線の線と、左の閉じた入口
            s = stroke(&[(150.0, -20.0), (150.0, 500.0), (330.0, 800.0)], LINE);
            s.extend(capsule((330.0, -20.0), (330.0, 380.0), LINE));
            dash(&mut s, 600.0);
            s.extend(capsule((874.0, -20.0), (874.0, 800.0), LINE));
        }
        6 | 7 => {
            // ⛠ ⛡ 左の入口の制限: 左上の黒い角と車線の線
            s = poly(&[(150.0, 800.0), (480.0, 800.0), (480.0, 640.0), (300.0, 640.0), (300.0, 400.0), (150.0, 400.0)]);
            dash(&mut s, 620.0);
            if kind == 7 {
                s.extend(poly(&[(500.0, -20.0), (874.0, -20.0), (874.0, 340.0)]));
            } else {
                s.extend(capsule((874.0, -20.0), (874.0, 800.0), LINE));
            }
        }
        _ => {}
    }
    s
}

enum Right {
    /// 源柔の字そのものを、偏の右に入るよう横に縮める
    Scale(char),
    /// 源柔の字の x が split より右の輪郭 (旁) を、偏の右に入るよう横に縮める
    From(char, f64),
}

/// 偏 (源柔の字 host の x が split より左の輪郭) と旁を組んで漢字にする
fn kanji(b: &Base, (host, split): (char, f64), right: Right) -> Result<Shape> {
    let left = b.half(host, split, true)?;
    let (_, _, lx1, _) = bbox(&left);
    let r = match right {
        Right::Scale(ch) => b.glyph(ch)?,
        Right::From(ch, sp) => b.half(ch, sp, false)?,
    };
    let (rx0, _, rx1, _) = bbox(&r);
    // 偏の右に、源柔の字の右端 (980 ほど) まで
    let gap = 40.0;
    let (to0, to1) = (lx1 + gap, 975.0);
    let k = (to1 - to0) / (rx1 - rx0);
    let mut out = left;
    out.extend(place(r, rx0, 0.0, k, 1.0, to0, 0.0));
    Ok(out)
}

/// 楽器の略記: 半角の字を横に縮めて並べる。1マスに4字まで入る幅 (半角の半分) にそろえ、縦は字面の中心を保って縮める
pub fn compose(glyphs: &Glyphs, cmap: &Cmap, text: &str) -> Result<Shape> {
    let b = Base { glyphs, cmap };
    let n = text.chars().count() as f64;
    let width = 512.0 * COMPOSE_SX * n;
    let mut x = if text.starts_with('(') && !text.ends_with(')') {
        1024.0 - width // 左半分: 右に寄せて次のマスへ続ける
    } else if text.ends_with(')') && !text.starts_with('(') {
        0.0
    } else {
        (1024.0 - width) / 2.0
    };
    let mut out = vec![];
    for ch in text.chars() {
        out.extend(place(b.glyph(ch)?, 0.0, 0.0, COMPOSE_SX, COMPOSE_SY, x, 280.0 * (1.0 - COMPOSE_SY)));
        x += 512.0 * COMPOSE_SX;
    }
    Ok(out)
}

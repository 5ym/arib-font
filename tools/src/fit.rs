//! 格子から引いた等高線 (1 単位ごとの細かい折れ線) を、TrueType の輪郭 (直線と2次曲線) に当てはめる。
//!
//! 1. 格子の揺れを均す
//! 2. 直線を探す: 長さ LINE_MIN 以上で、点が弦から LINE_DEV より離れない区間 (最小二乗の直線にそろえる)
//! 3. 直線と直線の間 (丸めた角・元の字の曲線) は2次曲線で埋める: 両端の向き (隣の直線の向き) を保った
//!    2次曲線 1 本で離れが TOL 以内に収まらなければ、曲がりの半分のところで割って繰り返す
//! 4. 2つの off-curve の点の中点に十分近い on-curve の点は省く (TrueType は中点を補う)

use crate::round::Poly;
use crate::sfnt::{Contour, Pt};

/// 当てはめの許し (em 1024 の単位)。曲線が元の等高線から離れてよい距離
pub const TOL: f64 = 1.0;
/// 直線とみなす長さの下限と、点が弦から離れてよい距離
const LINE_MIN: f64 = 48.0;
const LINE_DEV: f64 = 0.35;

pub type V = (f64, f64);

fn sub(a: V, b: V) -> V {
    (a.0 - b.0, a.1 - b.1)
}
fn add(a: V, b: V) -> V {
    (a.0 + b.0, a.1 + b.1)
}
fn mul(a: V, k: f64) -> V {
    (a.0 * k, a.1 * k)
}
fn dot(a: V, b: V) -> f64 {
    a.0 * b.0 + a.1 * b.1
}
fn cross(a: V, b: V) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
fn len(a: V) -> f64 {
    a.0.hypot(a.1)
}
fn unit(a: V) -> V {
    let l = len(a).max(1e-12);
    (a.0 / l, a.1 / l)
}

/// 点 q と線分 ab の距離
fn seg_dist(q: V, a: V, b: V) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 < 1e-12 { 0.0 } else { (dot(sub(q, a), ab) / l2).clamp(0.0, 1.0) };
    len(sub(q, add(a, mul(ab, t))))
}

/// 点 q と直線 ab の距離
fn line_dist(q: V, a: V, b: V) -> f64 {
    let ab = sub(b, a);
    let l = len(ab);
    if l < 1e-9 { len(sub(q, a)) } else { cross(ab, sub(q, a)).abs() / l }
}

pub fn area(c: &[V]) -> f64 {
    let n = c.len();
    (0..n).map(|i| cross(c[i], c[(i + 1) % n])).sum::<f64>() / 2.0
}

pub enum Seg {
    Line(V),
    Quad(V, V),
}

/// 閉じた折れ線を輪郭にする。小さすぎるもの (面積 25 未満) は None
pub fn fit(raw: &Poly) -> Option<Contour> {
    if raw.len() < 3 || area(raw).abs() < 25.0 {
        return None;
    }
    // 1. 格子の揺れを均す ([1 2 1]/4 を 3 回。1 単位おきの点なので曲がりはほとんど縮まない)
    let mut p = raw.clone();
    let n = p.len();
    for _ in 0..12 {
        p = (0..n)
            .map(|i| {
                let (a, b, d) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
                ((a.0 + 2.0 * b.0 + d.0) / 4.0, (a.1 + 2.0 * b.1 + d.1) / 4.0)
            })
            .collect();
    }
    // 始点は一番曲がっているところ (直線の途中から始めると直線が 2 つに割れる)
    let turn = |i: usize| {
        let (a, b, c) = (p[(i + n - 3) % n], p[i], p[(i + 3) % n]);
        let (u, v) = (unit(sub(b, a)), unit(sub(c, b)));
        cross(u, v).atan2(dot(u, v)).abs()
    };
    let start = (0..n).max_by(|&a, &b| turn(a).total_cmp(&turn(b))).unwrap_or(0);
    p.rotate_left(start);
    // 閉じるために始点を最後にも置く
    p.push(p[0]);
    let m = p.len();

    // 2. 直線
    let ok = |i: usize, j: usize| (i + 1..j).all(|k| line_dist(p[k], p[i], p[j]) <= LINE_DEV);
    let mut lines: Vec<(usize, usize)> = vec![];
    let mut i = 0;
    while i + 2 < m {
        // 倍々に伸ばしてから二分で詰める
        let mut good = i + 1;
        let mut step = 2;
        while i + step < m && ok(i, i + step) {
            good = i + step;
            step *= 2;
        }
        let mut hi = (i + step).min(m);
        while hi - good > 1 {
            let mid = (good + hi) / 2;
            if ok(i, mid) {
                good = mid;
            } else {
                hi = mid;
            }
        }
        if len(sub(p[good], p[i])) >= LINE_MIN {
            lines.push((i, good));
            i = good;
        } else {
            i += 1;
        }
    }
    // 最小二乗の直線 (重心と向き) にそろえた端
    let fitted: Vec<(V, V, V)> = lines
        .iter()
        .map(|&(a, b)| {
            let k = (b - a + 1) as f64;
            let c = mul(p[a..=b].iter().fold((0.0, 0.0), |s, &q| add(s, q)), 1.0 / k);
            let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
            for &q in &p[a..=b] {
                let d = sub(q, c);
                sxx += d.0 * d.0;
                sxy += d.0 * d.1;
                syy += d.1 * d.1;
            }
            let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
            let mut dir = (ang.cos(), ang.sin());
            if dot(dir, sub(p[b], p[a])) < 0.0 {
                dir = mul(dir, -1.0);
            }
            let proj = |q: V| add(c, mul(dir, dot(sub(q, c), dir)));
            (proj(p[a]), proj(p[b]), dir)
        })
        .collect();
    // 間に曲線を挟まずに続く直線どうしが浅い角度で会うなら、なだらかな曲線を割ったもの: 直線をやめる
    let nl = lines.len();
    let mut keep = vec![true; nl];
    for k in 0..nl {
        let l = (k + 1) % nl;
        if nl < 2 {
            break;
        }
        let touching = lines[k].1 == lines[l].0 || (lines[k].1 == m - 1 && lines[l].0 == 0);
        if touching && dot(fitted[k].2, fitted[l].2) > 20f64.to_radians().cos() {
            keep[k] = false;
            keep[l] = false;
        }
    }
    let idx: Vec<usize> = (0..nl).filter(|&k| keep[k]).collect();

    // 3. 並べる
    let mut segs: Vec<Seg> = vec![];
    let first: V;
    if idx.is_empty() {
        // 直線が無い (丸など): 一周を曲線で
        first = p[0];
        let t = unit(sub(p[3.min(m - 1)], p[m - 1 - 3.min(m - 1)]));
        fit_curve(&p[1..m - 1], p[0], t, p[0], t, &mut segs);
    } else {
        let (a0, _) = lines[idx[0]];
        first = fitted[idx[0]].0;
        for (q, &k) in idx.iter().enumerate() {
            let (_, b) = lines[k];
            let (_, e, dir) = fitted[k];
            segs.push(Seg::Line(e));
            let nk = idx[(q + 1) % idx.len()];
            let (na, _) = lines[nk];
            let (ns, _, ndir) = fitted[nk];
            // b から次の直線の始まり na まで (最後は一周して a0 まで)
            let pts: Vec<V> = if q + 1 < idx.len() { p[(b + 1).min(na)..na].to_vec() } else { p[(b + 1).min(m - 1)..m - 1].iter().chain(p[..a0].iter()).copied().collect() };
            fit_curve(&pts, e, dir, ns, ndir, &mut segs);
        }
    }

    // 4. 点にする
    let mut c: Contour = vec![Pt { x: first.0, y: first.1, on: true }];
    for s in &segs {
        match *s {
            Seg::Line(e) => c.push(Pt { x: e.0, y: e.1, on: true }),
            Seg::Quad(k, e) => {
                c.push(Pt { x: k.0, y: k.1, on: false });
                c.push(Pt { x: e.0, y: e.1, on: true });
            }
        }
    }
    c.pop(); // 始点に戻った点
    Some(tidy(c))
}

/// 2次曲線の点
fn quad_at(p0: V, p1: V, p2: V, t: f64) -> V {
    let u = 1.0 - t;
    add(add(mul(p0, u * u), mul(p1, 2.0 * u * t)), mul(p2, t * t))
}

/// 点の並び pts (両端 p0・p2 を除く) と2次曲線の一番大きな離れ
fn quad_err(pts: &[V], p0: V, p1: V, p2: V) -> f64 {
    let ns = 16 + pts.len() / 4;
    let samp: Vec<V> = (0..=ns).map(|i| quad_at(p0, p1, p2, i as f64 / ns as f64)).collect();
    let total: f64 = (1..=pts.len()).map(|i| len(sub(*pts.get(i).unwrap_or(&p2), pts[i - 1]))).sum::<f64>() + len(sub(pts.first().copied().unwrap_or(p2), p0));
    let mut acc = 0.0;
    let mut prev = p0;
    let mut worst: f64 = 0.0;
    for &q in pts {
        acc += len(sub(q, prev));
        prev = q;
        let t = if total > 0.0 { acc / total } else { 0.5 };
        let c = (t * ns as f64) as isize;
        let mut best = f64::MAX;
        for k in (c - 4).max(0)..(c + 4).min(ns as isize) {
            best = best.min(seg_dist(q, samp[k as usize], samp[k as usize + 1]));
        }
        worst = worst.max(best);
    }
    worst
}

/// p0 (向き t0) から p2 (向き t1) までを、間の点 pts に沿う2次曲線でつなぐ
pub fn fit_curve(pts: &[V], p0: V, t0: V, p2: V, t1: V, out: &mut Vec<Seg>) {
    let chord = sub(p2, p0);
    let cl = len(chord);
    let dev = pts.iter().map(|&q| line_dist(q, p0, p2)).fold(0.0, f64::max);
    // 直線で足りるのは、点が弦に沿い、両端の向きも弦とそろうとき (向きがずれたまま直線にすると継ぎ目が折れる)
    let along = |t: V| cl > 0.0 && dot(t, mul(chord, 1.0 / cl)) > 3f64.to_radians().cos();
    if pts.len() < 3 || (cl > 0.0 && dev <= TOL * 0.5 && along(t0) && along(t1)) {
        if cl > 1e-9 {
            out.push(Seg::Line(p2));
        }
        return;
    }
    // 2本の接線の交わり
    let den = cross(t0, t1);
    if den.abs() > 0.05 {
        let a = cross(chord, t1) / den;
        let b = cross(t0, chord) / den;
        let reach = 2.0 * cl.max(1.0);
        if a > 0.0 && b > 0.0 && a < reach && b < reach {
            let p1 = add(p0, mul(t0, a));
            if quad_err(pts, p0, p1, p2) <= TOL {
                out.push(Seg::Quad(p1, p2));
                return;
            }
        }
    }
    if pts.len() < 8 {
        // 細かすぎる: 真ん中を通る折れ線
        out.push(Seg::Line(pts[pts.len() / 2]));
        out.push(Seg::Line(p2));
        return;
    }
    // 割る: 曲がりの合計の半分のところ (曲がりが小さければ弦から一番離れたところ)
    let mut dirs = vec![unit(sub(pts[0], p0))];
    for w in pts.windows(2) {
        dirs.push(unit(sub(w[1], w[0])));
    }
    dirs.push(unit(sub(p2, pts[pts.len() - 1])));
    let turns: Vec<f64> = dirs.windows(2).map(|d| cross(d[0], d[1]).atan2(dot(d[0], d[1])).abs()).collect();
    let total: f64 = turns.iter().sum();
    let k = 3usize;
    let mut s = if total > 10f64.to_radians() {
        let mut acc = 0.0;
        let mut at = pts.len() / 2;
        for (i, t) in turns.iter().enumerate() {
            acc += t;
            if acc >= total / 2.0 {
                at = i;
                break;
            }
        }
        at
    } else {
        (0..pts.len()).max_by(|&a, &b| line_dist(pts[a], p0, p2).total_cmp(&line_dist(pts[b], p0, p2))).unwrap()
    };
    s = s.clamp(k, pts.len() - 1 - k);
    let tm = unit(sub(pts[(s + k).min(pts.len() - 1)], pts[s.saturating_sub(k)]));
    fit_curve(&pts[..s], p0, t0, pts[s], tm, out);
    fit_curve(&pts[s + 1..], pts[s], tm, p2, t1, out);
}

/// 要らない点を省く: 2つの off-curve の中点に近い on-curve と、まっすぐ続く直線の間の点
pub fn tidy(c: Contour) -> Contour {
    let mut c = c;
    loop {
        let n = c.len();
        if n < 4 {
            return c;
        }
        let mut drop = None;
        for i in 0..n {
            let (a, b, d) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
            if !b.on {
                continue;
            }
            let mid = ((a.x + d.x) / 2.0, (a.y + d.y) / 2.0);
            if !a.on && !d.on && len(sub((b.x, b.y), mid)) <= 0.75 {
                drop = Some(i);
                break;
            }
            if a.on && d.on && line_dist((b.x, b.y), (a.x, a.y), (d.x, d.y)) <= 0.5 && dot(sub((b.x, b.y), (a.x, a.y)), sub((d.x, d.y), (b.x, b.y))) > 0.0 {
                drop = Some(i);
                break;
            }
        }
        match drop {
            Some(i) => {
                c.remove(i);
            }
            None => return c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(r: f64, step: f64) -> Poly {
        let n = (2.0 * std::f64::consts::PI * r / step) as usize;
        // 時計回り (TrueType の外側)
        (0..n).map(|i| {
            let a = -(i as f64) * 2.0 * std::f64::consts::PI / n as f64;
            (512.0 + r * a.cos(), 389.0 + r * a.sin())
        }).collect()
    }

    #[test]
    fn circle_is_few_points() {
        let c = fit(&circle(458.0, 2.0)).unwrap();
        eprintln!("circle: {} points, on {}", c.len(), c.iter().filter(|p| p.on).count());
        assert!(c.len() <= 20, "{}", c.len());
    }

    #[test]
    fn rounded_rect_is_lines_and_corners() {
        // 角を半径 41 で丸めた四角
        let mut p: Poly = vec![];
        let (x0, y0, x1, y1, r) = (100.0, 0.0, 600.0, 400.0, 41.0);
        let corner = |p: &mut Poly, cx: f64, cy: f64, a0: f64| {
            for k in 0..=20 {
                let a = (a0 - k as f64 * 4.5f64).to_radians();
                p.push((cx + r * a.cos(), cy + r * a.sin()));
            }
        };
        corner(&mut p, x1 - r, y1 - r, 90.0);
        corner(&mut p, x1 - r, y0 + r, 0.0);
        corner(&mut p, x0 + r, y0 + r, -90.0);
        corner(&mut p, x0 + r, y1 - r, -180.0);
        // 辺を 2 単位おきに埋める
        let mut dense: Poly = vec![];
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let n = (len(sub(b, a)) / 2.0).ceil().max(1.0) as usize;
            for k in 0..n {
                dense.push(add(a, mul(sub(b, a), k as f64 / n as f64)));
            }
        }
        let c = fit(&dense).unwrap();
        eprintln!("rrect: {} points, on {}", c.len(), c.iter().filter(|p| p.on).count());
        assert!(c.len() <= 20, "{}", c.len());
    }
}

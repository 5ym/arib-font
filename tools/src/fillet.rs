//! 元の字の輪郭の角を、輪郭のまま丸める (角に円弧を入れる)。
//!
//! 出っ張った角は半径 R1、へこんだ角は半径 R2 の円弧にする。円弧は2次曲線で近づける
//! (曲がりが 60° を超える角は2本)。角の両側の辺は、円弧が触れるところまで縮める。
//! 辺が短くて半径どおりの円弧が入らないとき (細い線の端など) は、辺の半分まで使える半径に小さくする。
//! 細い線の端は両側の角が辺を半分ずつ使うので、線の太さの半円になる。
//!
//! 元の字 (BIZ UDゴシック) は輪郭が重ならないように整えてあるので、輪郭どうしを気にせず角ごとに丸められる。
//! 元の点はそのまま運ぶので、点の数は角 1 つにつき 2〜3 増えるだけ。

use crate::fit;
use crate::round::{R1, R2};
use crate::sfnt::{Contour, Pt};

type V = (f64, f64);

/// これより曲がりの小さい角は丸めない (なめらかなつなぎ目)
const MIN_TURN: f64 = 8.0;
/// 円弧を 2 本の2次曲線にする曲がり
const SPLIT_TURN: f64 = 60.0;
/// 円弧を 2 本にするのは半径がこれ以上のときだけ (小さな円弧は 1 本でも丸く見える。へこんだ角の R2 は 1 本)
const SPLIT_RADIUS: f64 = 20.0;

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

/// 直線か2次曲線
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub p0: V,
    pub c: Option<V>,
    pub p1: V,
}

impl Segment {
    pub fn at(&self, t: f64) -> V {
        match self.c {
            None => add(self.p0, mul(sub(self.p1, self.p0), t)),
            Some(c) => {
                let u = 1.0 - t;
                add(add(mul(self.p0, u * u), mul(c, 2.0 * u * t)), mul(self.p1, t * t))
            }
        }
    }
    /// t での向き (長さ 1)
    pub fn dir(&self, t: f64) -> V {
        let d = match self.c {
            None => sub(self.p1, self.p0),
            Some(c) => {
                let d = add(mul(sub(c, self.p0), 1.0 - t), mul(sub(self.p1, c), t));
                if len(d) < 1e-9 { sub(self.p1, self.p0) } else { d }
            }
        };
        mul(d, 1.0 / len(d).max(1e-12))
    }
    /// u から v までの部分
    pub fn part(&self, u: f64, v: f64) -> Segment {
        let c = self.c.map(|c| {
            let (a, b, d) = ((1.0 - u) * (1.0 - v), (1.0 - u) * v + u * (1.0 - v), u * v);
            add(add(mul(self.p0, a), mul(c, b)), mul(self.p1, d))
        });
        Segment { p0: self.at(u), c, p1: self.at(v) }
    }
    /// 長さ (曲線は細かい折れ線で測る)
    pub fn length(&self) -> f64 {
        self.arc_to(1.0)
    }
    fn arc_to(&self, t: f64) -> f64 {
        match self.c {
            None => len(sub(self.p1, self.p0)) * t,
            Some(_) => {
                let n = 32;
                let mut s = 0.0;
                let mut prev = self.p0;
                for i in 1..=n {
                    let p = self.at(t * i as f64 / n as f64);
                    s += len(sub(p, prev));
                    prev = p;
                }
                s
            }
        }
    }
    /// 始め (from_end なら終わり) から長さ l のところの t
    fn t_at(&self, l: f64, from_end: bool) -> f64 {
        let total = self.length();
        if total <= 0.0 {
            return if from_end { 1.0 } else { 0.0 };
        }
        let target = if from_end { total - l } else { l };
        if self.c.is_none() {
            return (target / total).clamp(0.0, 1.0);
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..30 {
            let mid = (lo + hi) / 2.0;
            if self.arc_to(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    }
}

/// TrueType の点の並びを区間にする (off-curve が続くところは中点を on-curve にする)
pub fn segments(c: &Contour) -> Vec<Segment> {
    if c.len() < 2 {
        return vec![];
    }
    let mut pts: Vec<Pt> = c.clone();
    match pts.iter().position(|q| q.on) {
        Some(i) => pts.rotate_left(i),
        None => {
            let m = Pt { x: (pts[0].x + pts[1].x) / 2.0, y: (pts[0].y + pts[1].y) / 2.0, on: true };
            pts.insert(1, m);
            pts.rotate_left(1);
        }
    }
    let mut out = vec![];
    let mut cur = (pts[0].x, pts[0].y);
    let mut ctrl: Option<V> = None;
    for q in pts[1..].iter().chain(std::iter::once(&pts[0])) {
        let p = (q.x, q.y);
        if q.on {
            out.push(Segment { p0: cur, c: ctrl.take(), p1: p });
            cur = p;
        } else {
            if let Some(c1) = ctrl {
                let mid = mul(add(c1, p), 0.5);
                out.push(Segment { p0: cur, c: Some(c1), p1: mid });
                cur = mid;
            }
            ctrl = Some(p);
        }
    }
    out.retain(|s| len(sub(s.p1, s.p0)) > 1e-6);
    out
}

/// 2 本の直線 (点 a を通る向き ta、点 b を通る向き tb) の交わり
fn meet(a: V, ta: V, b: V, tb: V) -> Option<V> {
    let den = cross(ta, tb);
    if den.abs() < 1e-9 {
        return None;
    }
    let s = cross(sub(b, a), tb) / den;
    Some(add(a, mul(ta, s)))
}

/// 角の円弧 (a から向き ta、b へ向き tb、曲がり turn 度)。2次曲線 1 本か 2 本の点 (a は含まず b は含む)
fn arc(a: V, ta: V, b: V, tb: V, turn: f64, radius: f64) -> Vec<Pt> {
    let on = |p: V| Pt { x: p.0, y: p.1, on: true };
    let off = |p: V| Pt { x: p.0, y: p.1, on: false };
    let Some(c) = meet(a, ta, b, tb) else { return vec![on(b)] };
    if turn <= SPLIT_TURN || radius < SPLIT_RADIUS {
        return vec![off(c), on(b)];
    }
    // 円の有理2次曲線の真ん中 (重み cos(曲がり/2))、そこでの向きは ab に平行
    let w = (turn.to_radians() / 2.0).cos();
    let m = mul(add(add(a, mul(c, 2.0 * w)), b), 1.0 / (2.0 + 2.0 * w));
    let tm = mul(sub(b, a), 1.0 / len(sub(b, a)).max(1e-12));
    match (meet(a, ta, m, tm), meet(m, tm, b, tb)) {
        (Some(c1), Some(c2)) => vec![off(c1), on(m), off(c2), on(b)],
        _ => vec![off(c), on(b)],
    }
}


/// まとめた円弧 (細い線の端の半円など): 円を 60° ずつの2次曲線で (a は含まず b は含む)。
/// 円は a で向き ta に触れ、b を通るもの (b での向きは tb に近い)
fn arc_n(a: V, ta: V, b: V, tb: V, turn: f64) -> Vec<Pt> {
    let on = |p: V| Pt { x: p.0, y: p.1, on: true };
    let off = |p: V| Pt { x: p.0, y: p.1, on: false };
    let chord = len(sub(b, a));
    let phi = turn.to_radians();
    if chord < 1e-6 || phi < 1e-6 {
        return vec![on(b)];
    }
    let r = chord / (2.0 * (phi / 2.0).sin());
    // 右へ曲がる (出っ張った角): 中心は進む向きの右
    let right = cross(ta, sub(b, a)) < 0.0;
    let nrm = if right { (ta.1, -ta.0) } else { (-ta.1, ta.0) };
    let o = add(a, mul(nrm, r));
    let k = ((turn / SPLIT_TURN).ceil() as usize).max(1);
    let step = phi / k as f64 * if right { -1.0 } else { 1.0 };
    let a0 = (a.1 - o.1).atan2(a.0 - o.0);
    let mut out = vec![];
    let mut prev = a;
    let mut tprev = ta;
    for i in 1..=k {
        let ang = a0 + step * i as f64;
        let p = if i == k { b } else { (o.0 + r * ang.cos(), o.1 + r * ang.sin()) };
        let tp = if i == k { tb } else if right { (ang.sin(), -ang.cos()) } else { (-ang.sin(), ang.cos()) };
        match meet(prev, tprev, p, tp) {
            Some(c) => out.push(off(c)),
            None => {}
        }
        out.push(on(p));
        prev = p;
        tprev = tp;
    }
    out
}

/// 輪郭の角を丸める
pub fn round_contour(c: &Contour) -> Contour {
    let segs = segments(c);
    let n = segs.len();
    if n < 2 {
        return c.clone();
    }
    // 角ごとに: (向きの曲がり, 縮める長さ)。角 i は区間 i-1 の終わりと区間 i の始め
    let lens: Vec<f64> = segs.iter().map(Segment::length).collect();
    let mut cut = vec![0.0; n];
    let mut turns = vec![0.0; n];
    let mut convex = vec![false; n];
    for i in 0..n {
        let (prev, next) = (&segs[(i + n - 1) % n], &segs[i]);
        let (tin, tout) = (prev.dir(1.0), next.dir(0.0));
        let turn = cross(tin, tout).atan2(dot(tin, tout)).to_degrees();
        if turn.abs() < MIN_TURN {
            continue;
        }
        // TrueType は塗るところが進む向きの右。右へ曲がる角が出っ張った角
        let r = if turn < 0.0 { R1 } else { R2 };
        let want = r * (turn.abs().to_radians() / 2.0).tan();
        let room = (lens[(i + n - 1) % n] / 2.0).min(lens[i] / 2.0);
        cut[i] = want.min(room);
        turns[i] = turn.abs();
        convex[i] = turn < 0.0;
    }
    // 両端の出っ張った角で使い切る直線の辺 (細い線の端): 2 つの角の円弧を 1 つの半円にまとめる
    let merged: Vec<bool> = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            segs[i].c.is_none() && cut[i] > 0.0 && cut[j] > 0.0 && cut[i] + cut[j] >= lens[i] - 0.5 && turns[i] > 0.0 && turns[j] > 0.0 && convex[i] && convex[j]
        })
        .collect();
    // まとめた辺とその前の辺のどちらでもない辺から始める (一周の切れ目をまとめたところに置かない)
    let start = (0..n).find(|&r| !merged[r] && !merged[(r + n - 1) % n]);
    let (start, merged) = match start {
        Some(r) => (r, merged),
        None => (0, vec![false; n]),
    };
    // 区間を縮めて、角に円弧を入れる
    let mut out: Contour = vec![];
    let on = |p: V| Pt { x: p.0, y: p.1, on: true };
    let radius = |k: usize| cut[k] / (turns[k].to_radians() / 2.0).tan();
    let mut k = 0;
    while k < n {
        let i = (start + k) % n;
        let s = &segs[i];
        let (c0, c1) = (cut[i], cut[(i + 1) % n]);
        let t0 = if c0 > 0.0 { s.t_at(c0, false) } else { 0.0 };
        let t1 = if c1 > 0.0 { s.t_at(c1, true) } else { 1.0 };
        let part = s.part(t0, t1.max(t0));
        if out.is_empty() {
            out.push(on(part.p0));
        }
        if let Some(c) = part.c {
            out.push(Pt { x: c.0, y: c.1, on: false });
        }
        out.push(on(part.p1));
        // 次の角
        let j = (i + 1) % n;
        k += 1;
        if cut[j] > 0.0 {
            if merged[j] && k < n {
                // 角 j・辺 j・角 j+1 をまとめて 1 つの円弧に
                let j2 = (j + 1) % n;
                let ns = &segs[j2];
                let tb = ns.t_at(cut[j2], false);
                out.extend(arc_n(part.p1, s.dir(t1), ns.at(tb), ns.dir(tb), turns[j] + turns[j2]));
                k += 1; // 辺 j は使い切った
                // 次は辺 j2 から (その始めは cut[j2] だけ縮める)
                continue;
            }
            let ns = &segs[j];
            let tb = ns.t_at(cut[j], false);
            out.extend(arc(part.p1, s.dir(t1), ns.at(tb), ns.dir(tb), turns[j], radius(j)));
        }
    }
    // 同じ所に続く on-curve (使い切った辺の両端) と、始点に戻った点を省く
    let same = |a: &Pt, b: &Pt| a.on && b.on && (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6;
    let mut dedup: Contour = vec![];
    for p in out {
        if dedup.last().is_none_or(|l| !same(l, &p)) {
            dedup.push(p);
        }
    }
    while dedup.len() > 1 && same(&dedup[0], &dedup[dedup.len() - 1]) {
        dedup.pop();
    }
    let out = dedup;
    fit::tidy(out)
}

/// 字の全部の輪郭の角を丸める
pub fn round_shape(s: &[Contour]) -> Vec<Contour> {
    s.iter().map(round_contour).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
        // 時計回り (y 上向き)
        [(x0, y0), (x0, y1), (x1, y1), (x1, y0)].iter().map(|&(x, y)| Pt { x, y, on: true }).collect()
    }

    #[test]
    fn square_corners_get_r1() {
        let c = round_contour(&rect(0.0, 0.0, 500.0, 300.0));
        // 角 4 つ × (on, off, on, off) で 16 点 (円弧の真ん中の on は中点なので省ける)
        assert!(c.len() <= 16, "{}", c.len());
        let xs: Vec<f64> = c.iter().filter(|p| p.on && p.y == 0.0).map(|p| p.x).collect();
        assert!(xs.iter().any(|&x| (x - R1).abs() < 1e-6), "{xs:?}");
    }

    #[test]
    fn thin_end_is_half_circle() {
        // 太さ 60 の横線の端: 両側の角が 30 ずつ使う
        let c = round_contour(&rect(0.0, 0.0, 400.0, 60.0));
        // 左の端はちょうど x = 0 に触れ、そこは y = 30 (線の真ん中)
        let poly = &crate::round::flatten(std::slice::from_ref(&c), 0.01)[0];
        let tip = poly.iter().copied().fold((f64::MAX, 0.0), |a, p| if p.0 < a.0 { p } else { a });
        assert!(tip.0.abs() < 0.05 && (tip.1 - 30.0).abs() < 1.0, "{tip:?}");
    }

    #[test]
    fn concave_corner_gets_r2() {
        // L 字 (時計回り)
        let pts = [(0.0, 0.0), (0.0, 300.0), (100.0, 300.0), (100.0, 100.0), (300.0, 100.0), (300.0, 0.0)];
        let c: Contour = pts.iter().map(|&(x, y)| Pt { x, y, on: true }).collect();
        let r = round_contour(&c);
        assert!(r.iter().any(|p| p.on && (p.x - 100.0).abs() < 1e-6 && (p.y - (100.0 + R2)).abs() < 1e-6));
    }
}

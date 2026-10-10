//! 試し: 角ゴシックを機械で丸くする (build からは使わない。元を替えるか決めるための見本づくり)。
//!
//!   round <font> <out.ttf> <r1> <r2> [--text FILE] [--repertoire] [--wght 400] [--res 1]
//!       r1 = 出っ張った角の丸み、r2 = へこんだ角の丸み (どちらも em の 1/1000)
//!   coverage <font>...     denpa の字の一覧がどれだけ入っているか、半角・全角の幅
//!
//! 丸め方 (新しい crate は足さない。距離変換と等高線は std だけで書く):
//! 1. 字を em 1/1000 の格子に塗る (非ゼロ規則なので重なり・向きの乱れはここで消える)
//! 2. 「適応オープニング」: 中の点から外までの距離 D を測り、D ≥ r の点を半径 r で膨らませたもの
//!    (ふつうのオープニング = 出っ張った角が半径 r になる) に、細い線 (D < r) の芯の点を半径 D で膨らませたものを足す。
//!    ふつうのオープニングは幅 2r より細い線を消してしまうが、こうすると細い線は消えずに端が半円になる。
//!    どちらも元の字の内側にしか描かないので線は太らない
//! 3. へこんだ角は、地 (字の外) に同じことをする (= クロージング)。細い隙間は埋まらない。
//!    その前後で、線の継ぎ目に残る細い切れ込み・閉じ込められた小さな穴 (元の字の中) は埋め戻す
//! 4. 距離の場から等高線を引き (marching squares)、少し均してから Douglas-Peucker で間引き、
//!    なめらかなところは off-curve の点にする (2次の B スプライン。ちゃんとした曲線の当てはめはまだ)
//!
//! 弱いところ: r が線の太さの半分を超えると、線の継ぎ目に小さなこぶ・欠けが出る (r40 ではほぼ出ない)。
//! ■ などの記号も丸まる (外す字の一覧が要る)。点が元の 2 倍ほどに増える。1 字 0.2〜0.3 秒 (格子の距離変換)
//!
//! ヒンティングは無し。出すフォントは見本用 (glyf・cmap・hmtx などだけ。名前は元のまま)。

use crate::err::{bail, Ctx, Result};
use crate::sfnt::{self, put_u16, Contour, Glyphs, Pt};
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::MetadataProvider;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::time::Instant;
use write_fonts::read::{FontRef, TableProvider};
use write_fonts::types::{GlyphId, Tag};

const BIG: f64 = 1e20;

// ---------------------------------------------------------------- 輪郭を折れ線に

struct Flat {
    s: f64,
    tol: f64,
    polys: Vec<Vec<(f64, f64)>>,
    cur: (f64, f64),
}

impl Flat {
    fn push(&mut self, p: (f64, f64)) {
        if let Some(c) = self.polys.last_mut() {
            c.push(p);
        }
        self.cur = p;
    }
}

impl OutlinePen for Flat {
    fn move_to(&mut self, x: f32, y: f32) {
        self.polys.push(vec![]);
        self.push((x as f64 * self.s, y as f64 * self.s));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push((x as f64 * self.s, y as f64 * self.s));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.cur, (cx as f64 * self.s, cy as f64 * self.s), (x as f64 * self.s, y as f64 * self.s));
        let dd = ((p0.0 - 2.0 * p1.0 + p2.0).powi(2) + (p0.1 - 2.0 * p1.1 + p2.1).powi(2)).sqrt();
        let n = ((dd / (4.0 * self.tol)).sqrt().ceil() as usize).clamp(1, 100);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            self.push((u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1));
        }
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let s = self.s;
        let (p0, p1, p2, p3) = (self.cur, (cx0 as f64 * s, cy0 as f64 * s), (cx1 as f64 * s, cy1 as f64 * s), (x as f64 * s, y as f64 * s));
        let d1 = ((p0.0 - 2.0 * p1.0 + p2.0).powi(2) + (p0.1 - 2.0 * p1.1 + p2.1).powi(2)).sqrt();
        let d2 = ((p1.0 - 2.0 * p2.0 + p3.0).powi(2) + (p1.1 - 2.0 * p2.1 + p3.1).powi(2)).sqrt();
        let n = ((3.0 * d1.max(d2) / (4.0 * self.tol)).sqrt().ceil() as usize).clamp(1, 100);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push((a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1));
        }
    }
    fn close(&mut self) {}
}

// ---------------------------------------------------------------- 格子

struct Grid {
    w: usize,
    h: usize,
}

impl Grid {
    /// 画素の中心 (x0 + i + 0.5, y0 + j + 0.5) が中なら true (非ゼロ規則)
    fn fill(&self, polys: &[Vec<(f64, f64)>], x0: f64, y0: f64) -> Vec<bool> {
        let (w, h) = (self.w, self.h);
        let mut rows: Vec<Vec<(f64, i32)>> = vec![vec![]; h];
        for p in polys {
            let n = p.len();
            for k in 0..n {
                let (a, b) = (p[k], p[(k + 1) % n]);
                if a.1 == b.1 {
                    continue;
                }
                let (lo, hi, dir) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
                let j0 = (lo.1 - y0 - 0.5).ceil().max(0.0) as usize;
                let j1 = ((hi.1 - y0 - 0.5).ceil().max(0.0) as usize).min(h);
                for (j, row) in rows.iter_mut().enumerate().take(j1).skip(j0) {
                    let yc = y0 + j as f64 + 0.5;
                    row.push((lo.0 + (yc - lo.1) * (hi.0 - lo.0) / (hi.1 - lo.1), dir));
                }
            }
        }
        let mut m = vec![false; w * h];
        for (j, row) in rows.iter_mut().enumerate() {
            row.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut wind = 0;
            for k in 0..row.len().saturating_sub(1) {
                wind += row[k].1;
                if wind != 0 {
                    let i0 = (row[k].0 - x0 - 0.5).ceil().max(0.0) as usize;
                    let i1 = ((row[k + 1].0 - x0 - 0.5).ceil().max(0.0) as usize).min(w);
                    for v in &mut m[j * w + i0.min(w)..j * w + i1.max(i0.min(w))] {
                        *v = true;
                    }
                }
            }
        }
        m
    }

    /// 一番近い feat の画素の中心までの距離 (Felzenszwalb の距離変換)。feat が無ければ BIG
    fn edt(&self, feat: &[bool]) -> Vec<f64> {
        let (w, h) = (self.w, self.h);
        let n = w.max(h);
        let mut f = vec![0.0; n];
        let mut d = vec![0.0; n];
        let mut v = vec![0usize; n];
        let mut z = vec![0.0; n + 1];
        let mut g: Vec<f64> = feat.iter().map(|&b| if b { 0.0 } else { BIG }).collect();
        for i in 0..w {
            for j in 0..h {
                f[j] = g[j * w + i];
            }
            dt1(&f[..h], &mut d[..h], &mut v, &mut z);
            for j in 0..h {
                g[j * w + i] = d[j];
            }
        }
        for j in 0..h {
            f[..w].copy_from_slice(&g[j * w..j * w + w]);
            dt1(&f[..w], &mut d[..w], &mut v, &mut z);
            g[j * w..j * w + w].copy_from_slice(&d[..w]);
        }
        g.iter().map(|&x| if x >= BIG { BIG } else { x.sqrt() }).collect()
    }

    /// 適応オープニング。返すのは中で正・外で負の場 (0 が新しい輪郭)
    fn open(&self, mask: &[bool], r: f64) -> Vec<f64> {
        let (w, h) = (self.w, self.h);
        let bg: Vec<bool> = mask.iter().map(|&m| !m).collect();
        let dbg = self.edt(&bg);
        let dd: Vec<f64> = (0..w * h).map(|i| if mask[i] { dbg[i] - 0.5 } else { 0.0 }).collect();
        let e: Vec<bool> = (0..w * h).map(|i| mask[i] && dd[i] >= r).collect();
        let de = self.edt(&e);
        let mut phi: Vec<f64> = de.iter().map(|&d| if d >= BIG { -BIG } else { r + 0.5 - d }).collect();
        // 細い線の芯: 周りより距離が伸びない点 (角へ向かう枝は 90° の角で 0.7、60° で 0.5 伸びるので入らない)。
        // 鋭い角 (斜めの線の端) へ向かう枝は伸びが小さいので、近く (半径 r/2) の一番太いところの 0.7 倍より細い点も外す
        let near = max_filter(&dd, w, h, (r / 2.0).ceil() as usize);
        let mut done = vec![0.0f64; w * h];
        for j in 0..h {
            for i in 0..w {
                let c = j * w + i;
                let dc = dd[c];
                if !mask[c] || dc < 1.0 || dc >= r + 0.5 || dc < 0.7 * near[c].min(r) {
                    continue;
                }
                let mut ridge = true;
                'n: for dj in -1i32..=1 {
                    for di in -1i32..=1 {
                        let (ni, nj) = (i as i32 + di, j as i32 + dj);
                        if (di == 0 && dj == 0) || ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                            continue;
                        }
                        let len = if di != 0 && dj != 0 { std::f64::consts::SQRT_2 } else { 1.0 };
                        if (dd[nj as usize * w + ni as usize] - dc) / len >= 0.4 {
                            ridge = false;
                            break 'n;
                        }
                    }
                }
                if !ridge {
                    continue;
                }
                let rho = dc.min(r);
                // すぐ隣でほぼ同じ円を描いたなら飛ばす (芯は 1〜3 画素の幅があり、同じ円を何度も描くことになる)
                let mut dup = false;
                for qj in j.saturating_sub(1)..=(j + 1).min(h - 1) {
                    for qi in i.saturating_sub(1)..=(i + 1).min(w - 1) {
                        dup |= done[qj * w + qi] >= rho - 0.3;
                    }
                }
                if dup {
                    continue;
                }
                done[c] = rho;
                let k = (rho + 1.5).ceil() as i32;
                for qj in (j as i32 - k).max(0)..=(j as i32 + k).min(h as i32 - 1) {
                    for qi in (i as i32 - k).max(0)..=(i as i32 + k).min(w as i32 - 1) {
                        let dist = (((qi - i as i32).pow(2) + (qj - j as i32).pow(2)) as f64).sqrt();
                        let q = qj as usize * w + qi as usize;
                        let val = rho - dist;
                        if val > phi[q] {
                            phi[q] = val;
                        }
                    }
                }
            }
        }
        phi
    }

    /// bg のうち格子の縁につながらない塊で、ほとんど (7 割) が mask の中か、小さい (max_area 未満で半分が中) ものを埋める (bg を false にする)
    fn fill_inner_holes(&self, bg: &mut [bool], mask: &[bool], max_area: f64) {
        let (w, h) = (self.w, self.h);
        let mut seen = vec![false; w * h];
        let mut stack = vec![];
        for s in 0..w * h {
            if !bg[s] || seen[s] {
                continue;
            }
            let mut comp = vec![];
            let (mut edge, mut inside) = (false, 0usize);
            seen[s] = true;
            stack.push(s);
            while let Some(p) = stack.pop() {
                comp.push(p);
                let (i, j) = (p % w, p / w);
                edge |= i == 0 || j == 0 || i == w - 1 || j == h - 1;
                inside += mask[p] as usize;
                let mut go = |q: usize| {
                    if bg[q] && !seen[q] {
                        seen[q] = true;
                        stack.push(q);
                    }
                };
                if i > 0 {
                    go(p - 1);
                }
                if i + 1 < w {
                    go(p + 1);
                }
                if j > 0 {
                    go(p - w);
                }
                if j + 1 < h {
                    go(p + w);
                }
            }
            let n = comp.len() as f64;
            if !edge && (inside as f64 >= 0.7 * n || (n < max_area && inside as f64 >= 0.5 * n)) {
                for p in comp {
                    bg[p] = false;
                }
            }
        }
    }

    /// 場の 0 の等高線。中 (正) を右に見て回る輪郭 (y 上向きで外側は時計回り = TrueType の向き)
    fn contours(&self, phi: &[f64]) -> Vec<Vec<(f64, f64)>> {
        let (w, h) = (self.w, self.h);
        let at = |i: usize, j: usize| phi[j * w + i];
        // 点の名前: 横の辺 (i,j)-(i+1,j) は 2*(j*w+i)、縦の辺 (i,j)-(i,j+1) は 2*(j*w+i)+1
        let mut next: HashMap<usize, usize> = HashMap::new();
        let mut pos: HashMap<usize, (f64, f64)> = HashMap::new();
        for j in 0..h - 1 {
            for i in 0..w - 1 {
                // 左下から反時計回り
                let c = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                let v = c.map(|(a, b)| at(a, b));
                let ins = v.map(|x| x > 0.0);
                if ins.iter().all(|&b| b) || ins.iter().all(|&b| !b) {
                    continue;
                }
                let eid = [2 * (j * w + i), 2 * (j * w + i + 1) + 1, 2 * ((j + 1) * w + i), 2 * (j * w + i) + 1];
                let mut cross: Vec<(usize, bool)> = vec![]; // (辺の番号, 入る)
                for k in 0..4 {
                    let (a, b) = (k, (k + 1) % 4);
                    if ins[a] != ins[b] {
                        let t = v[a] / (v[a] - v[b]);
                        let p = (c[a].0 as f64 + t * (c[b].0 as f64 - c[a].0 as f64), c[a].1 as f64 + t * (c[b].1 as f64 - c[a].1 as f64));
                        pos.insert(eid[k], p);
                        cross.push((k, ins[b]));
                    }
                }
                if cross.len() == 2 {
                    let (a, b) = if cross[0].1 { (cross[0].0, cross[1].0) } else { (cross[1].0, cross[0].0) };
                    next.insert(eid[a], eid[b]);
                } else {
                    // 鞍点: 真ん中が中ならつながる
                    let center = v.iter().sum::<f64>() / 4.0 > 0.0;
                    let ent: Vec<usize> = cross.iter().filter(|x| x.1).map(|x| x.0).collect();
                    let ext: Vec<usize> = cross.iter().filter(|x| !x.1).map(|x| x.0).collect();
                    for &e in &ent {
                        // 反時計回りで次の出口 (離れる) か、その次 (つながる)
                        let mut cand: Vec<usize> = ext.iter().map(|&x| (x + 4 - e) % 4).collect();
                        cand.sort();
                        let pick = if center { cand[cand.len() - 1] } else { cand[0] };
                        next.insert(eid[e], eid[(e + pick) % 4]);
                    }
                }
            }
        }
        let mut out = vec![];
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut keys: Vec<usize> = next.keys().copied().collect();
        keys.sort();
        for s in keys {
            if seen.contains(&s) {
                continue;
            }
            let mut loop_ = vec![];
            let mut cur = s;
            while seen.insert(cur) {
                loop_.push(pos[&cur]);
                match next.get(&cur) {
                    Some(&n) => cur = n,
                    None => break,
                }
            }
            if loop_.len() >= 3 {
                out.push(loop_);
            }
        }
        out
    }
}

fn dt1(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let mut k: isize = -1;
    for q in 0..n {
        if f[q] >= BIG {
            continue;
        }
        let qf = q as f64;
        let mut s = 0.0;
        while k >= 0 {
            let p = v[k as usize];
            let pf = p as f64;
            s = ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * qf - 2.0 * pf);
            if s <= z[k as usize] {
                k -= 1;
            } else {
                break;
            }
        }
        if k < 0 {
            k = 0;
            v[0] = q;
            z[0] = -BIG;
            z[1] = BIG;
        } else {
            k += 1;
            v[k as usize] = q;
            z[k as usize] = s;
            z[k as usize + 1] = BIG;
        }
    }
    if k < 0 {
        d.iter_mut().for_each(|x| *x = BIG);
        return;
    }
    let mut j = 0;
    for (q, out) in d.iter_mut().enumerate() {
        let qf = q as f64;
        while z[j + 1] < qf {
            j += 1;
        }
        let p = v[j];
        *out = (qf - p as f64).powi(2) + f[p];
    }
}

// ---------------------------------------------------------------- 折れ線を2次の輪郭に

fn area(c: &[(f64, f64)]) -> f64 {
    let n = c.len();
    (0..n).map(|i| c[i].0 * c[(i + 1) % n].1 - c[(i + 1) % n].0 * c[i].1).sum::<f64>() / 2.0
}

/// 閉じた折れ線を Douglas-Peucker で間引く
fn simplify(p: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    let n = p.len();
    let far = (1..n)
        .max_by(|&a, &b| {
            let da = (p[a].0 - p[0].0).hypot(p[a].1 - p[0].1);
            let db = (p[b].0 - p[0].0).hypot(p[b].1 - p[0].1);
            da.total_cmp(&db)
        })
        .unwrap_or(0);
    let mut keep = vec![false; n + 1];
    keep[0] = true;
    keep[far] = true;
    keep[n] = true;
    let at = |i: usize| p[i % n];
    let mut stack = vec![(0, far), (far, n)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (at(a), at(b));
        let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
        let len = dx.hypot(dy);
        let mut best = (0.0, a);
        for i in a + 1..b {
            let q = at(i);
            let d = if len < 1e-9 { (q.0 - pa.0).hypot(q.1 - pa.1) } else { ((q.0 - pa.0) * dy - (q.1 - pa.1) * dx).abs() / len };
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    (0..n).filter(|&i| keep[i]).map(|i| p[i]).collect()
}

/// なめらかな点 (曲がりが小さく、両隣の辺が短い) を off-curve にする。TrueType の2次 B スプライン
fn to_contour(p: &[(f64, f64)], lmax: f64) -> Contour {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
            let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
            let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
            let cos = (u.0 * v.0 + u.1 * v.1) / (lu * lv).max(1e-9);
            let smooth = cos > 50f64.to_radians().cos() && lu < lmax && lv < lmax;
            Pt { x: b.0, y: b.1, on: !smooth }
        })
        .collect()
}

/// 1字を丸める。polys は格子の単位 (px)。返すのはフォントの単位
fn round_glyph(polys: &[Vec<(f64, f64)>], s: f64, r1: f64, r2: f64) -> Vec<Contour> {
    let pts = polys.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    if x0 > x1 {
        return vec![];
    }
    let m = r1.max(r2) + 4.0;
    let (x0, y0) = ((x0 - m).floor(), (y0 - m).floor());
    let g = Grid { w: (x1 - x0 + m).ceil() as usize + 1, h: (y1 - y0 + m).ceil() as usize + 1 };
    let mask = g.fill(polys, x0, y0);
    let mut phi = g.open(&mask, r1);
    if r2 > 0.0 {
        // 芯の円と太いところの円の継ぎ目に細い切れ込みや穴が残るので埋め戻す: 元の字の中にある幅 8 より細いへこみ
        // (半径 4 のクロージングを元の字と重ねる。字と字の隙間は元の字の外なので埋まらない) と、全部が元の字の中にある穴
        let k = 4.0;
        let o1: Vec<bool> = phi.iter().map(|&v| v > 0.0).collect();
        let dil: Vec<bool> = g.edt(&o1).iter().map(|&d| d <= k).collect();
        let undil: Vec<bool> = dil.iter().map(|&b| !b).collect();
        let closed = g.edt(&undil);
        let mut bg: Vec<bool> = (0..o1.len()).map(|i| !(o1[i] || (mask[i] && closed[i] > k))).collect();
        g.fill_inner_holes(&mut bg, &mask, 4.0 * r2 * r2);
        phi = g.open(&bg, r2).into_iter().map(|v| -v).collect();
        // 地の細い通り道が消えて閉じ込められた小さな穴 (元の字の中) も埋める
        let mut bgf: Vec<bool> = phi.iter().map(|&v| v <= 0.0).collect();
        let before = bgf.clone();
        g.fill_inner_holes(&mut bgf, &mask, 4.0 * r2 * r2);
        for i in 0..phi.len() {
            if before[i] && !bgf[i] {
                phi[i] = 1.0;
            }
        }
    }
    let mut out = vec![];
    for c in g.contours(&phi) {
        // 格子の点 (i,j) は画素の中心 x0+i+0.5
        let c: Vec<(f64, f64)> = c.iter().map(|&(i, j)| (i + x0 + 0.5, j + y0 + 0.5)).collect();
        // 芯の円の継ぎ目にできる小さな点・穴 (半径 3 画素ほど) は捨てる
        if area(&c).abs() < 25.0 {
            continue;
        }
        // 格子の揺れを均す ([1 2 1]/4 を 3 回。1 画素おきの点なので曲がりはほとんど縮まない)
        let mut c = c;
        let n = c.len();
        for _ in 0..3 {
            c = (0..n)
                .map(|i| {
                    let (a, b, d) = (c[(i + n - 1) % n], c[i], c[(i + 1) % n]);
                    ((a.0 + 2.0 * b.0 + d.0) / 4.0, (a.1 + 2.0 * b.1 + d.1) / 4.0)
                })
                .collect();
        }
        let tol = 0.8; // 間引きの許し (格子の単位)。0.5 だと 2 割ほど大きく、1.2 だと 400px で角ばりが見える
        let c = simplify(&c, tol);
        if c.len() < 3 {
            continue;
        }
        let c: Vec<(f64, f64)> = c.iter().map(|&(x, y)| (x / s, y / s)).collect();
        out.push(to_contour(&c, 64.0 / s));
    }
    out
}

// ---------------------------------------------------------------- フォント

fn wght_location(font: &FontRef, wght: f32) -> Location {
    font.axes().location([("wght", wght)])
}

pub fn run(args: &[String]) -> Result<()> {
    if args.len() < 5 {
        bail!("round <font> <out.ttf> <r1> <r2> [--text FILE] [--repertoire] [--wght 400] [--res 1]");
    }
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    let data = fs::read(&args[1]).ctx(|| args[1].clone())?;
    let font = FontRef::new(&data)?;
    let (r1, r2): (f64, f64) = (args[3].parse()?, args[4].parse()?);
    let res: f64 = opt("--res").map_or(Ok(1.0), |s| s.parse())?;
    let wght: f32 = opt("--wght").map_or(Ok(400.0), |s| s.parse())?;
    let upm = font.head()?.units_per_em() as f64;
    let s = res * 1000.0 / upm;
    let loc = wght_location(&font, wght);
    let cmap = font.charmap();

    let mut chars: BTreeSet<u32> = BTreeSet::new();
    if let Some(f) = opt("--text") {
        chars.extend(fs::read_to_string(f)?.chars().filter(|c| !c.is_control()).map(|c| c as u32));
    }
    if args.iter().any(|a| a == "--repertoire") {
        let r = crate::repertoire::load(Path::new(crate::SRC))?;
        chars.extend(r.codepoints.iter().filter(|c| !r.pua.contains_key(c)));
    }
    let mut map: BTreeMap<u32, u32> = BTreeMap::new(); // 字 → 元の glyph
    for &c in &chars {
        if let Some(g) = cmap.map(c) {
            map.insert(c, g.to_u32());
        }
    }
    let mut src: Vec<u32> = vec![0];
    src.extend(map.values().copied().collect::<BTreeSet<_>>());
    let new_id: BTreeMap<u32, u32> = src.iter().enumerate().map(|(i, &g)| (g, i as u32)).collect();

    let t0 = Instant::now();
    let outlines = font.outline_glyphs();
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::from(&loc));
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut results: Vec<(Vec<u8>, u16)> = vec![(vec![], 0); src.len()];
    std::thread::scope(|sc| {
        let chunks: Vec<_> = results.chunks_mut(src.len().div_ceil(threads)).enumerate().collect();
        for (ci, chunk) in chunks {
            let (src, outlines, metrics, loc) = (&src, &outlines, &metrics, &loc);
            let base = ci * src.len().div_ceil(threads);
            sc.spawn(move || {
                for (k, slot) in chunk.iter_mut().enumerate() {
                    let gid = GlyphId::new(src[base + k]);
                    let adv = metrics.advance_width(gid).unwrap_or(0.0).round() as u16;
                    let mut pen = Flat { s, tol: 0.1, polys: vec![], cur: (0.0, 0.0) };
                    if let Some(o) = outlines.get(gid) {
                        let _ = o.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::from(loc)), &mut pen);
                    }
                    let contours = round_glyph(&pen.polys, s, r1 * res, r2 * res);
                    *slot = (sfnt::encode_simple(&contours), adv);
                }
            });
        }
    });
    let elapsed = t0.elapsed();

    let lsb = results.iter().map(|(d, _)| if d.is_empty() { 0 } else { sfnt::i16_at(d, 2) }).collect();
    let advance = results.iter().map(|r| r.1).collect();
    let glyphs = Glyphs { data: results.into_iter().map(|r| r.0).collect(), advance, lsb };
    let ttf = build_font(&font, &glyphs, map.iter().map(|(&c, g)| (c, new_id[g])))?;
    fs::write(&args[2], &ttf)?;
    let woff2 = ttf2woff2::encode(&ttf, ttf2woff2::BrotliQuality::from(11u8)).map_err(|e| format!("woff2: {e}"))?;
    println!(
        "{}: {} 字 ({} glyph)、r1={r1} r2={r2}、{:.1} 秒 ({threads} スレッド)、ttf {} バイト、woff2 {} バイト",
        args[2],
        map.len(),
        glyphs.len(),
        elapsed.as_secs_f64(),
        ttf.len(),
        woff2.len()
    );
    Ok(())
}

/// 丸めた字だけの TrueType を組む (元の OS/2・name・hhea・head は運ぶ)
fn build_font(base: &FontRef, glyphs: &Glyphs, cmap: impl Iterator<Item = (u32, u32)>) -> Result<Vec<u8>> {
    let mut b = write_fonts::FontBuilder::new();
    let (glyf, loca, hmtx) = glyphs.tables();
    b.add_raw(Tag::new(b"glyf"), glyf);
    b.add_raw(Tag::new(b"loca"), loca);
    b.add_raw(Tag::new(b"hmtx"), hmtx);
    let n = glyphs.len() as u16;
    let (mp, mc) = glyphs.max_points_contours();
    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    put_u16(&mut maxp, 4, n);
    put_u16(&mut maxp, 6, mp);
    put_u16(&mut maxp, 8, mc);
    put_u16(&mut maxp, 14, 2);
    b.add_raw(Tag::new(b"maxp"), maxp);
    let mut head = sfnt::table(base, b"head")?.to_vec();
    sfnt::put_i16(&mut head, 50, 1);
    b.add_raw(Tag::new(b"head"), head);
    let mut hhea = sfnt::table(base, b"hhea")?.to_vec();
    put_u16(&mut hhea, 34, n);
    b.add_raw(Tag::new(b"hhea"), hhea);
    for t in [b"OS/2", b"name"] {
        b.add_raw(Tag::new(t), sfnt::table(base, t)?.to_vec());
    }
    let mut post = sfnt::table(base, b"post")?[..32].to_vec();
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    b.add_raw(Tag::new(b"post"), post);
    let cm = write_fonts::tables::cmap::Cmap::from_mappings(cmap.filter_map(|(c, g)| char::from_u32(c).map(|ch| (ch, GlyphId::new(g)))))
        .map_err(|e| format!("cmap: {e:?}"))?;
    b.add_table(&cm)?;
    Ok(b.build())
}

// ---------------------------------------------------------------- 字の一覧との照らし合わせ

pub fn coverage(fonts: &[String]) -> Result<()> {
    let r = crate::repertoire::load(Path::new(crate::SRC))?;
    let rep: Vec<u32> = r.codepoints.iter().copied().filter(|c| !r.pua.contains_key(c)).collect();
    let gj = fs::read(Path::new(crate::SRC).join("GenJyuuGothic-Monospace-Regular.ttf"))?;
    let gj = sfnt::charmap(&FontRef::new(&gj)?);
    println!("一覧 {} 字 (私用領域の同じ字・並べた字 {} を除く)", rep.len(), r.pua.len());
    println!("| フォント | upm | glyph | 入っている | 無い | うち源柔にある | A | ｱ | あ | 一 | 幅が 0.5/1em でない字 |");
    for path in fonts {
        let data = fs::read(path).ctx(|| path.clone())?;
        let font = FontRef::new(&data)?;
        let upm = font.head()?.units_per_em() as f32;
        let cmap = font.charmap();
        let loc = wght_location(&font, 400.0);
        let m = font.glyph_metrics(Size::unscaled(), LocationRef::from(&loc));
        let adv = |c: u32| cmap.map(c).and_then(|g| m.advance_width(g)).map(|a| a / upm);
        let em = |c: u32| adv(c).map_or("-".to_string(), |a| format!("{a:.2}"));
        let missing: Vec<u32> = rep.iter().copied().filter(|&c| cmap.map(c).is_none()).collect();
        let gap: Vec<u32> = missing.iter().copied().filter(|c| gj.contains_key(c)).collect();
        let oddv: Vec<u32> = rep.iter().copied().filter(|&c| adv(c).is_some_and(|a| (a - 0.5).abs() > 0.01 && (a - 1.0).abs() > 0.01)).collect();
        let odd = oddv.len();
        let stem = Path::new(path).file_stem().unwrap().to_string_lossy().to_string();
        println!(
            "| {stem} | {upm} | {} | {} | {} | {} | {} | {} | {} | {} | {odd} |",
            font.maxp()?.num_glyphs(),
            rep.len() - missing.len(),
            missing.len(),
            gap.len(),
            em(0x41),
            em(0xFF71),
            em(0x3042),
            em(0x4E00)
        );
        fs::create_dir_all("out")?;
        fs::write(format!("out/width-{stem}.txt"), oddv.iter().map(|&c| format!("U+{c:04X} {} {:.3}
", char::from_u32(c).unwrap_or('?'), adv(c).unwrap_or(0.0))).collect::<String>())?;
        fs::write(format!("out/missing-{stem}.txt"), format!("# 無い字 (源柔にある字の前に *)\n{}", missing.iter().map(|&c| format!("{}U+{c:04X} {}\n", if gj.contains_key(&c) { "*" } else { " " }, char::from_u32(c).unwrap_or('?'))).collect::<String>()))?;
    }
    Ok(())
}

/// 正方形 (半径 k) の中の最大 (縦横に分けて、単調な列で 1 画素あたり定数時間)
fn max_filter(v: &[f64], w: usize, h: usize, k: usize) -> Vec<f64> {
    fn line(src: &[f64], dst: &mut [f64], k: usize) {
        let n = src.len();
        let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        let mut next = 0;
        for i in 0..n {
            while next < n && next <= i + k {
                while q.back().is_some_and(|&b| src[b] <= src[next]) {
                    q.pop_back();
                }
                q.push_back(next);
                next += 1;
            }
            while q.front().is_some_and(|&f| f + k < i) {
                q.pop_front();
            }
            dst[i] = src[q[0]];
        }
    }
    let mut t = vec![0.0; w * h];
    for j in 0..h {
        line(&v[j * w..j * w + w], &mut t[j * w..j * w + w], k);
    }
    let mut o = vec![0.0; w * h];
    let (mut a, mut b) = (vec![0.0; h], vec![0.0; h]);
    for i in 0..w {
        for j in 0..h {
            a[j] = t[j * w + i];
        }
        line(&a, &mut b, k);
        for j in 0..h {
            o[j * w + i] = b[j];
        }
    }
    o
}

//! 描いた字 (draw.rs) と部品を組む字 (parts.rs) を丸める。図形や部品が重なるので、格子 (1 マス RES 単位) に塗ってから角を丸め、
//! 輪郭に戻す (戻すのは fit.rs)。元の字 (BIZ UDゴシック) は輪郭のまま丸める (fillet.rs)。
//! 新しい crate は使わない (距離変換・等高線は std だけ)。
//!
//! 1. 字を格子に塗る (非ゼロ規則なので重なり・向きの乱れはここで消える)。部品を組む字 (parts.rs) は
//!    部品ごとに塗って切り抜き、縮めて細った線を元の太さに戻してから (横に縮めたら横に、縦なら縦に膨らませる)、重ねる
//! 2. 「適応オープニング」で出っ張った角を半径 R1 に: 中の点から外までの距離 D を測り、D ≥ R1 の点を半径 R1 で
//!    膨らませたもの (ふつうのオープニング) に、細い線 (D < R1) の芯の点を半径 D で膨らませたものを足す。
//!    ふつうのオープニングは幅 2·R1 より細い線を消してしまうが、こうすると細い線は消えずに端が半円になる。
//!    どちらも元の字の内側にしか描かないので線は太らない。芯の円を並べた縁は格子の分だけ揺れるので、
//!    丸めた形を 1 画素膨らませて元の字と重ね、角と線の端のほかは元の字の縁に戻す
//! 3. へこんだ角は、地 (字の外) に同じことをして半径 R2 に (= クロージング)。細い隙間は埋まらない。
//!    その前後で、線の継ぎ目に残る細い切れ込み・閉じ込められた小さな穴 (元の字の中) は埋め戻す
//! 4. 距離の場の 0 の等高線を引く (marching squares)
//!
//! 弱いところ: R1 が線の太さの半分を大きく超えると、線の継ぎ目に小さなこぶ・欠けが出る (R1 = 41 ではほぼ出ない)。

use crate::sfnt::{Contour, Pt};

/// 出っ張った角の丸み (em の 4%。源柔ゴシックとほぼ同じ丸さ)
pub const R1: f64 = 41.0;
/// へこんだ角の丸み
pub const R2: f64 = 12.0;
/// 格子の 1 画素の大きさ (em 1024 の単位)。2 で em を 512 マスに (1 にしても見た目は変わらず、4 倍遅い)
pub const RES: f64 = 2.0;

const BIG: f64 = 1e20;

pub type Poly = Vec<(f64, f64)>;

/// 2次の輪郭を折れ線にする (tol: 弦と曲線の離れの上限)
pub fn flatten(contours: &[Contour], tol: f64) -> Vec<Poly> {
    let mut out = vec![];
    for c in contours {
        if c.len() < 2 {
            continue;
        }
        // 最初の点を on-curve にする (全部 off-curve なら最初の2つの中点を足す)
        let mut pts = c.clone();
        match pts.iter().position(|q| q.on) {
            Some(i) => pts.rotate_left(i),
            None => {
                let m = Pt { x: (pts[0].x + pts[1].x) / 2.0, y: (pts[0].y + pts[1].y) / 2.0, on: true };
                pts.insert(1, m);
                pts.rotate_left(1);
            }
        }
        let mut poly = vec![(pts[0].x, pts[0].y)];
        let mut cur = (pts[0].x, pts[0].y);
        let mut ctrl: Option<(f64, f64)> = None;
        let quad = |poly: &mut Poly, p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)| {
            let dd = ((p0.0 - 2.0 * p1.0 + p2.0).powi(2) + (p0.1 - 2.0 * p1.1 + p2.1).powi(2)).sqrt();
            let k = ((dd / (4.0 * tol)).sqrt().ceil() as usize).clamp(1, 200);
            for i in 1..=k {
                let t = i as f64 / k as f64;
                let u = 1.0 - t;
                poly.push((u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1));
            }
        };
        for q in pts[1..].iter().chain(std::iter::once(&pts[0])) {
            let p = (q.x, q.y);
            if q.on {
                match ctrl.take() {
                    Some(c1) => quad(&mut poly, cur, c1, p),
                    None => poly.push(p),
                }
                cur = p;
            } else {
                if let Some(c1) = ctrl {
                    let mid = ((c1.0 + p.0) / 2.0, (c1.1 + p.1) / 2.0);
                    quad(&mut poly, cur, c1, mid);
                    cur = mid;
                }
                ctrl = Some(p);
            }
        }
        poly.pop(); // 始点に戻った点
        if poly.len() >= 3 {
            out.push(poly);
        }
    }
    out
}

/// 格子。1 画素は RES 単位。画素 (i, j) の中心は ((x0 + i + 0.5) * RES, (y0 + j + 0.5) * RES)
pub struct Grid {
    pub x0: f64,
    pub y0: f64,
    pub w: usize,
    pub h: usize,
}

impl Grid {
    /// 範囲 (x0, y0, x1, y1) を余白 m つきで覆う格子
    pub fn covering(b: (f64, f64, f64, f64), m: f64) -> Grid {
        let (x0, y0) = (((b.0 - m) / RES).floor(), ((b.1 - m) / RES).floor());
        Grid { x0, y0, w: ((b.2 + m) / RES - x0).ceil() as usize + 1, h: ((b.3 + m) / RES - y0).ceil() as usize + 1 }
    }

    /// 折れ線を塗る (非ゼロ規則)。画素の中心が中なら true
    pub fn fill(&self, polys: &[Poly]) -> Vec<bool> {
        let (w, h) = (self.w, self.h);
        let mut rows: Vec<Vec<(f64, i32)>> = vec![vec![]; h];
        for p in polys {
            let n = p.len();
            for k in 0..n {
                let (a, b) = (p[k], p[(k + 1) % n]);
                let (a, b) = ((a.0 / RES, a.1 / RES), (b.0 / RES, b.1 / RES));
                if a.1 == b.1 {
                    continue;
                }
                let (lo, hi, dir) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
                let j0 = (lo.1 - self.y0 - 0.5).ceil().max(0.0) as usize;
                let j1 = ((hi.1 - self.y0 - 0.5).ceil().max(0.0) as usize).min(h);
                for (j, row) in rows.iter_mut().enumerate().take(j1).skip(j0) {
                    let yc = self.y0 + j as f64 + 0.5;
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
                    let i0 = ((row[k].0 - self.x0 - 0.5).ceil().max(0.0) as usize).min(w);
                    let i1 = ((row[k + 1].0 - self.x0 - 0.5).ceil().max(0.0) as usize).min(w);
                    for v in &mut m[j * w + i0..j * w + i1.max(i0)] {
                        *v = true;
                    }
                }
            }
        }
        m
    }

    /// 一番近い feat の画素の中心までの距離 (Meijster の距離変換。整数だけで計算する)。feat が無ければ BIG
    fn edt(&self, feat: &[bool]) -> Vec<f64> {
        let (w, h) = (self.w, self.h);
        let inf = (w + h) as i64;
        // 縦: 同じ列で一番近い feat までの距離 (行ごとに上から・下から)
        let mut g: Vec<i64> = vec![0; w * h];
        for i in 0..w {
            g[i] = if feat[i] { 0 } else { inf };
        }
        for j in 1..h {
            let (prev, cur) = g.split_at_mut(j * w);
            let prev = &prev[(j - 1) * w..];
            for i in 0..w {
                cur[i] = if feat[j * w + i] { 0 } else { (prev[i] + 1).min(inf) };
            }
        }
        for j in (0..h - 1).rev() {
            let (cur, next) = g.split_at_mut((j + 1) * w);
            let cur = &mut cur[j * w..];
            for i in 0..w {
                if next[i] + 1 < cur[i] {
                    cur[i] = next[i] + 1;
                }
            }
        }
        // 横: 下の包絡線
        let mut out = vec![BIG; w * h];
        let mut s = vec![0usize; w];
        let mut t = vec![0i64; w];
        for j in 0..h {
            let row = &g[j * w..j * w + w];
            let f = |x: i64, i: usize| (x - i as i64) * (x - i as i64) + row[i] * row[i];
            let sep = |i: usize, u: usize| {
                let (ii, uu) = (i as i64, u as i64);
                (uu * uu - ii * ii + row[u] * row[u] - row[i] * row[i]).div_euclid(2 * (uu - ii))
            };
            let mut q: isize = 0;
            s[0] = 0;
            t[0] = 0;
            for u in 1..w {
                while q >= 0 && f(t[q as usize], s[q as usize]) > f(t[q as usize], u) {
                    q -= 1;
                }
                if q < 0 {
                    q = 0;
                    s[0] = u;
                } else {
                    let wv = 1 + sep(s[q as usize], u);
                    if wv < w as i64 {
                        q += 1;
                        s[q as usize] = u;
                        t[q as usize] = wv;
                    }
                }
            }
            let lim = inf * inf;
            for u in (0..w).rev() {
                let d2 = f(u as i64, s[q as usize]);
                out[j * w + u] = if d2 >= lim { BIG } else { (d2 as f64).sqrt() };
                if u as i64 == t[q as usize] {
                    q -= 1;
                }
            }
        }
        out
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
        // 中心からの距離の表
        let tw = (r + 3.0).ceil() as usize;
        let table: Vec<f64> = (0..tw * tw).map(|k| (((k / tw).pow(2) + (k % tw).pow(2)) as f64).sqrt()).collect();
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
                // 近くでほぼ同じ円を描いたなら飛ばす (芯は 1〜3 画素の幅があり、同じ円を何度も描くことになる)。
                // 近くは半径の 1/6 (最大 5 画素): 円を 5 画素おきに並べても、半径 40 の線の縁の波は 0.1 画素
                let near_k = ((rho / 6.0) as usize).clamp(1, 5);
                let mut dup = false;
                for qj in j.saturating_sub(near_k)..=(j + near_k).min(h - 1) {
                    for qi in i.saturating_sub(near_k)..=(i + near_k).min(w - 1) {
                        dup |= done[qj * w + qi] >= rho - 0.3;
                    }
                }
                if dup {
                    continue;
                }
                done[c] = rho;
                let k = (rho + 1.5).ceil() as i32;
                for qj in (j as i32 - k).max(0)..=(j as i32 + k).min(h as i32 - 1) {
                    let trow = &table[(qj - j as i32).unsigned_abs() as usize * tw..];
                    let row = qj as usize * w;
                    for qi in (i as i32 - k).max(0)..=(i as i32 + k).min(w as i32 - 1) {
                        let q = row + qi as usize;
                        let val = rho - trow[(qi - i as i32).unsigned_abs() as usize];
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
        let mut comp = vec![];
        for s in 0..w * h {
            if !bg[s] || seen[s] {
                continue;
            }
            comp.clear();
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
                for &p in &comp {
                    bg[p] = false;
                }
            }
        }
    }

    /// 場の 0 の等高線 (格子の単位ではなくフォントの単位)。中 (正) を右に見て回る輪郭
    /// (y 上向きで外側は時計回り = TrueType の向き)
    pub fn contours(&self, phi: &[f64]) -> Vec<Poly> {
        let (w, h) = (self.w, self.h);
        let at = |i: usize, j: usize| phi[j * w + i];
        // 点の名前: 横の辺 (i,j)-(i+1,j) は 2*(j*w+i)、縦の辺 (i,j)-(i,j+1) は 2*(j*w+i)+1
        const NONE: u32 = u32::MAX;
        let mut next: Vec<u32> = vec![NONE; 2 * w * h];
        let mut pos: Vec<(f32, f32)> = vec![(0.0, 0.0); 2 * w * h];
        let mut starts: Vec<u32> = vec![];
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
                let mut cross: [(usize, bool); 4] = [(0, false); 4];
                let mut nc = 0;
                for k in 0..4 {
                    let (a, b) = (k, (k + 1) % 4);
                    if ins[a] != ins[b] {
                        let t = v[a] / (v[a] - v[b]);
                        let p = (c[a].0 as f64 + t * (c[b].0 as f64 - c[a].0 as f64), c[a].1 as f64 + t * (c[b].1 as f64 - c[a].1 as f64));
                        pos[eid[k]] = (p.0 as f32, p.1 as f32);
                        cross[nc] = (k, ins[b]);
                        nc += 1;
                    }
                }
                if nc == 2 {
                    let (a, b) = if cross[0].1 { (cross[0].0, cross[1].0) } else { (cross[1].0, cross[0].0) };
                    next[eid[a]] = eid[b] as u32;
                    starts.push(eid[a] as u32);
                } else {
                    // 鞍点: 真ん中が中ならつながる
                    let center = v.iter().sum::<f64>() / 4.0 > 0.0;
                    for &(e, inn) in &cross[..nc] {
                        if !inn {
                            continue;
                        }
                        // 反時計回りで次の出口 (離れる) か、その次 (つながる)
                        let mut cand: Vec<usize> = cross[..nc].iter().filter(|x| !x.1).map(|x| (x.0 + 4 - e) % 4).collect();
                        cand.sort();
                        let pick = if center { cand[cand.len() - 1] } else { cand[0] };
                        next[eid[e]] = eid[(e + pick) % 4] as u32;
                        starts.push(eid[e] as u32);
                    }
                }
            }
        }
        starts.sort();
        let mut out = vec![];
        let mut seen = vec![false; 2 * w * h];
        for s in starts {
            if seen[s as usize] {
                continue;
            }
            let mut lp = vec![];
            let mut cur = s;
            while cur != NONE && !seen[cur as usize] {
                seen[cur as usize] = true;
                let p = pos[cur as usize];
                // 格子の点 (i, j) は画素の中心 x0 + i + 0.5
                lp.push(((p.0 as f64 + self.x0 + 0.5) * RES, (p.1 as f64 + self.y0 + 0.5) * RES));
                cur = next[cur as usize];
            }
            if lp.len() >= 3 {
                out.push(lp);
            }
        }
        out
    }
}

/// 正方形 (半径 k) の中の最大 (縦横に分け、van Herk / Gil-Werman で 1 画素あたり定数回の比較)
fn max_filter(v: &[f64], w: usize, h: usize, k: usize) -> Vec<f64> {
    fn line(src: &[f64], dst: &mut [f64], k: usize, pre: &mut [f64], suf: &mut [f64]) {
        let n = src.len();
        let b = 2 * k + 1;
        for s in (0..n).step_by(b) {
            let e = (s + b).min(n);
            pre[s] = src[s];
            for i in s + 1..e {
                pre[i] = pre[i - 1].max(src[i]);
            }
            suf[e - 1] = src[e - 1];
            for i in (s..e - 1).rev() {
                suf[i] = suf[i + 1].max(src[i]);
            }
        }
        for (i, d) in dst.iter_mut().enumerate() {
            let (lo, hi) = (i.saturating_sub(k), (i + k).min(n - 1));
            // [lo, hi] は高々 2 つの塊にまたがる
            *d = suf[lo].max(pre[hi]);
            if hi / b == lo / b {
                *d = src[lo..=hi].iter().copied().fold(f64::MIN, f64::max);
            }
        }
    }
    let n = w.max(h);
    let (mut pre, mut suf) = (vec![0.0; n], vec![0.0; n]);
    let mut t = vec![0.0; w * h];
    for j in 0..h {
        line(&v[j * w..j * w + w], &mut t[j * w..j * w + w], k, &mut pre, &mut suf);
    }
    let mut o = vec![0.0; w * h];
    let (mut a, mut c) = (vec![0.0; h], vec![0.0; h]);
    for i in 0..w {
        for j in 0..h {
            a[j] = t[j * w + i];
        }
        line(&a, &mut c, k, &mut pre, &mut suf);
        for j in 0..h {
            o[j * w + i] = c[j];
        }
    }
    o
}

/// 横 (axis 0) か縦 (axis 1) に d 画素だけ膨らませる (d が負なら縮める)。部品を縮めて細った線を元の太さに戻す
pub fn grow(mask: &mut [bool], w: usize, h: usize, axis: usize, d: f64) {
    let k = d.abs().round() as usize;
    if k == 0 {
        return;
    }
    let shrink = d < 0.0;
    let (n, len, stride, step) = if axis == 0 { (h, w, w, 1) } else { (w, h, 1, w) };
    let mut line = vec![false; len];
    let mut dist = vec![usize::MAX; len];
    for a in 0..n {
        let base = a * stride;
        for (t, v) in line.iter_mut().enumerate() {
            *v = mask[base + t * step] != shrink; // 縮めるときは地を膨らませる
        }
        // 一番近い true までの距離 (前から・後ろから)
        let mut last = usize::MAX;
        for t in 0..len {
            if line[t] {
                last = t;
            }
            dist[t] = if last == usize::MAX { usize::MAX } else { t - last };
        }
        last = usize::MAX;
        for t in (0..len).rev() {
            if line[t] {
                last = t;
            }
            if last != usize::MAX {
                dist[t] = dist[t].min(last - t);
            }
        }
        for t in 0..len {
            mask[base + t * step] = (dist[t] <= k) != shrink;
        }
    }
}

/// 塗った字を丸めて、等高線 (フォントの単位) を返す。sharp なら丸めない (部品を組んだ字の角を残す)
pub fn round_mask(g: &Grid, mask: &[bool], sharp: bool) -> Vec<Poly> {
    if sharp {
        let bg: Vec<bool> = mask.iter().map(|&m| !m).collect();
        let d_in = g.edt(&bg);
        let d_out = g.edt(mask);
        let phi: Vec<f64> = (0..mask.len()).map(|i| if mask[i] { d_in[i] - 0.5 } else { 0.5 - d_out[i] }).collect();
        return g.contours(&phi);
    }
    let mut phi = g.open(mask, R1 / RES);
    // 芯の円と太いところの円の継ぎ目に細い切れ込みや穴が残るので埋め戻す: 元の字の中にある幅 8 より細いへこみ
    // (半径 4 のクロージングを元の字と重ねる。字と字の隙間は元の字の外なので埋まらない) と、全部が元の字の中にある穴
    let k = (4.0 / RES).max(1.0);
    // 丸めた形を少し (1 画素) 膨らませて元の字と重ねる: 角と線の端は丸めたもの、それ以外は元の字のまま
    // (細い線は芯の円を並べて作るので、縁が格子の分だけ揺れる。元の字の縁に戻す)
    let o1: Vec<bool> = phi.iter().zip(mask).map(|(&v, &m)| m && v + 1.0 > 0.0).collect();
    // (正方形のクロージング。細い切れ込みを埋めるだけなので丸でなくてよい)
    let mut closed = o1.clone();
    for (axis, d) in [(0, k), (1, k), (0, -k), (1, -k)] {
        grow(&mut closed, g.w, g.h, axis, d);
    }
    let mut bg: Vec<bool> = (0..o1.len()).map(|i| !(o1[i] || (mask[i] && closed[i]))).collect();
    g.fill_inner_holes(&mut bg, mask, 4.0 * R2 * R2 / (RES * RES));
    phi = g.open(&bg, R2 / RES).into_iter().map(|v| -v).collect();
    // 地の細い通り道が消えて閉じ込められた小さな穴 (元の字の中) も埋める
    let mut bgf: Vec<bool> = phi.iter().map(|&v| v <= 0.0).collect();
    let before = bgf.clone();
    g.fill_inner_holes(&mut bgf, mask, 4.0 * R2 * R2 / (RES * RES));
    for i in 0..phi.len() {
        if before[i] && !bgf[i] {
            phi[i] = 1.0;
        }
    }
    g.contours(&phi)
}

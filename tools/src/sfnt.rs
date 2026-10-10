//! TrueType (glyf) の字を書く小さな道具。表の組み立ては write-fonts、字の点の並びはここ。

use crate::err::{Ctx, Result};
use write_fonts::read::FontRef;
use write_fonts::types::Tag;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
    pub on: bool,
}
pub type Contour = Vec<Pt>;

pub fn table<'a>(font: &FontRef<'a>, tag: &[u8; 4]) -> Result<&'a [u8]> {
    Ok(font
        .table_data(Tag::new(tag))
        .ctx(|| format!("{} 表がありません", String::from_utf8_lossy(tag)))?
        .as_bytes())
}

pub fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}
pub fn i16_at(b: &[u8], o: usize) -> i16 {
    i16::from_be_bytes([b[o], b[o + 1]])
}
pub fn put_u16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_be_bytes());
}
pub fn put_i16(b: &mut [u8], o: usize, v: i16) {
    b[o..o + 2].copy_from_slice(&v.to_be_bytes());
}

/// 字のバイト列・送り幅・左の余白
pub struct Glyphs {
    pub data: Vec<Vec<u8>>,
    pub advance: Vec<u16>,
    pub lsb: Vec<i16>,
}

impl Glyphs {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// glyf・loca (長い形)・hmtx
    pub fn tables(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let mut glyf = vec![];
        let mut loca = vec![];
        for d in &self.data {
            loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
            glyf.extend_from_slice(d);
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        let mut hmtx = vec![];
        for (a, l) in self.advance.iter().zip(&self.lsb) {
            hmtx.extend_from_slice(&a.to_be_bytes());
            hmtx.extend_from_slice(&l.to_be_bytes());
        }
        (glyf, loca, hmtx)
    }

    /// maxp の点と輪郭の数の上限 (単純な字の分)
    pub fn max_points_contours(&self) -> (u16, u16) {
        let (mut p, mut c) = (0u16, 0u16);
        for d in &self.data {
            if d.is_empty() || i16_at(d, 0) < 0 {
                continue;
            }
            let nc = i16_at(d, 0) as usize;
            c = c.max(nc as u16);
            if nc > 0 {
                p = p.max(u16_at(d, 10 + (nc - 1) * 2) + 1);
            }
        }
        (p, c)
    }
}


/// fontTools の otRound と同じ (0.5 は上へ)
pub fn ot_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

/// 輪郭を単純な字 (命令なし) にする。fontTools の TTGlyphPen で書いたものと同じ点の並びにする:
/// 各輪郭は最初の on-curve の点から始め、始点と同じ終点は落とし、2点に満たない輪郭は捨てる
pub fn encode_simple(contours: &[Contour]) -> Vec<u8> {
    let mut pts: Vec<(i32, i32, bool)> = vec![];
    let mut ends: Vec<u16> = vec![];
    for c in contours {
        let mut r: Vec<(i32, i32, bool)> = c.iter().map(|q| (ot_round(q.x), ot_round(q.y), q.on)).collect();
        if let Some(first_on) = r.iter().position(|q| q.2) {
            r.rotate_left(first_on);
        }
        if r.len() > 1 && r[0].0 == r[r.len() - 1].0 && r[0].1 == r[r.len() - 1].1 {
            r.pop();
        }
        if r.len() < 2 {
            continue;
        }
        pts.extend(r);
        ends.push((pts.len() - 1) as u16);
    }
    if pts.is_empty() {
        return vec![];
    }
    let mut flags = vec![];
    let mut xb = vec![];
    let mut yb = vec![];
    let (mut px, mut py) = (0, 0);
    for &(x, y, on) in &pts {
        let mut f = if on { 1u8 } else { 0 };
        let (dx, dy) = (x - px, y - py);
        if dx == 0 {
            f |= 0x10;
        } else if dx.abs() < 256 {
            f |= 0x2 | if dx > 0 { 0x10 } else { 0 };
            xb.push(dx.unsigned_abs() as u8);
        } else {
            xb.extend_from_slice(&(dx as i16).to_be_bytes());
        }
        if dy == 0 {
            f |= 0x20;
        } else if dy.abs() < 256 {
            f |= 0x4 | if dy > 0 { 0x20 } else { 0 };
            yb.push(dy.unsigned_abs() as u8);
        } else {
            yb.extend_from_slice(&(dy as i16).to_be_bytes());
        }
        flags.push(f);
        (px, py) = (x, y);
    }
    let mut fb = vec![];
    let mut i = 0;
    while i < flags.len() {
        let mut r = 0;
        while i + r + 1 < flags.len() && flags[i + r + 1] == flags[i] && r < 255 {
            r += 1;
        }
        if r > 0 {
            fb.push(flags[i] | 0x8);
            fb.push(r as u8);
        } else {
            fb.push(flags[i]);
        }
        i += r + 1;
    }
    let mut out = vec![];
    out.extend_from_slice(&(ends.len() as i16).to_be_bytes());
    for v in [
        pts.iter().map(|q| q.0).min().unwrap(),
        pts.iter().map(|q| q.1).min().unwrap(),
        pts.iter().map(|q| q.0).max().unwrap(),
        pts.iter().map(|q| q.1).max().unwrap(),
    ] {
        out.extend_from_slice(&(v as i16).to_be_bytes());
    }
    for e in ends {
        out.extend_from_slice(&e.to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend(fb);
    out.extend(xb);
    out.extend(yb);
    out
}

//! TrueType (glyf) の字を読み書きする小さな道具。表の組み立ては write-fonts、
//! 字の点の読み書きはここ (元の字のバイト列はそのまま運び、足す字だけ作る)。

use crate::err::{bail, Ctx, Result};
use write_fonts::read::{FontRef, TableProvider};
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
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
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
    pub fn read(font: &FontRef) -> Result<Self> {
        let n = font.maxp()?.num_glyphs() as usize;
        let long = font.head()?.index_to_loc_format() == 1;
        let loca = table(font, b"loca")?;
        let glyf = table(font, b"glyf")?;
        let at = |i: usize| -> usize {
            if long {
                u32_at(loca, i * 4) as usize
            } else {
                u16_at(loca, i * 2) as usize * 2
            }
        };
        let data = (0..n).map(|i| glyf[at(i)..at(i + 1)].to_vec()).collect();
        let hmtx = table(font, b"hmtx")?;
        let nh = font.hhea()?.number_of_h_metrics() as usize;
        let mut advance = Vec::with_capacity(n);
        let mut lsb = Vec::with_capacity(n);
        for i in 0..n {
            if i < nh {
                advance.push(u16_at(hmtx, i * 4));
                lsb.push(i16_at(hmtx, i * 4 + 2));
            } else {
                advance.push(advance[nh - 1]);
                lsb.push(i16_at(hmtx, nh * 4 + (i - nh) * 2));
            }
        }
        Ok(Self { data, advance, lsb })
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 部品 (composite) をばらした輪郭。座標は小数のまま (丸めるのは書くとき)
    pub fn outline(&self, gid: usize) -> Result<Vec<Contour>> {
        self.outline_at(gid, 0)
    }

    fn outline_at(&self, gid: usize, depth: usize) -> Result<Vec<Contour>> {
        let b = &self.data[gid];
        if b.is_empty() {
            return Ok(vec![]);
        }
        if i16_at(b, 0) >= 0 {
            return Ok(decode_simple(b));
        }
        if depth > 16 {
            bail!("部品が深すぎます: glyph {gid}");
        }
        let mut out = vec![];
        let mut p = 10;
        loop {
            let flags = u16_at(b, p);
            let child = u16_at(b, p + 2) as usize;
            p += 4;
            let (dx, dy) = if flags & 0x1 != 0 {
                p += 4;
                (i16_at(b, p - 4) as f64, i16_at(b, p - 2) as f64)
            } else {
                p += 2;
                (b[p - 2] as i8 as f64, b[p - 1] as i8 as f64)
            };
            if flags & 0x2 == 0 {
                bail!("点で合わせる部品は扱えません: glyph {gid}");
            }
            if flags & 0x800 != 0 {
                bail!("SCALED_COMPONENT_OFFSET は扱えません: glyph {gid}");
            }
            let f2 = |o: usize| i16_at(b, o) as f64 / 16384.0;
            // 2x2 は xscale, scale01, scale10, yscale の順。x' = x*xx + y*xy, y' = x*yx + y*yy
            let (mut xx, mut yx, mut xy, mut yy) = (1.0, 0.0, 0.0, 1.0);
            if flags & 0x8 != 0 {
                xx = f2(p);
                yy = xx;
                p += 2;
            } else if flags & 0x40 != 0 {
                xx = f2(p);
                yy = f2(p + 2);
                p += 4;
            } else if flags & 0x80 != 0 {
                xx = f2(p);
                yx = f2(p + 2);
                xy = f2(p + 4);
                yy = f2(p + 6);
                p += 8;
            }
            for c in self.outline_at(child, depth + 1)? {
                out.push(
                    c.iter()
                        .map(|q| Pt { x: q.x * xx + q.y * xy + dx, y: q.x * yx + q.y * yy + dy, on: q.on })
                        .collect(),
                );
            }
            if flags & 0x20 == 0 {
                break;
            }
        }
        Ok(out)
    }

    /// 新しい字を足す。左の余白は字面の左端
    pub fn add(&mut self, contours: &[Contour], advance: u16) -> u32 {
        let data = encode_simple(contours);
        self.lsb.push(if data.is_empty() { 0 } else { i16_at(&data, 2) });
        self.data.push(data);
        self.advance.push(advance);
        (self.data.len() - 1) as u32
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

fn decode_simple(b: &[u8]) -> Vec<Contour> {
    let nc = i16_at(b, 0) as usize;
    let ends: Vec<usize> = (0..nc).map(|i| u16_at(b, 10 + i * 2) as usize).collect();
    let np = ends.last().map_or(0, |e| e + 1);
    let mut p = 10 + nc * 2;
    p += 2 + u16_at(b, p) as usize; // 命令
    let mut flags = Vec::with_capacity(np);
    while flags.len() < np {
        let f = b[p];
        p += 1;
        flags.push(f);
        if f & 0x8 != 0 {
            let r = b[p];
            p += 1;
            for _ in 0..r {
                flags.push(f);
            }
        }
    }
    let mut read = |short: u8, same: u8| -> Vec<i32> {
        let mut v = 0i32;
        flags
            .iter()
            .map(|&f| {
                if f & short != 0 {
                    let d = b[p] as i32;
                    p += 1;
                    v += if f & same != 0 { d } else { -d };
                } else if f & same == 0 {
                    v += i16_at(b, p) as i32;
                    p += 2;
                }
                v
            })
            .collect()
    };
    let xs = read(0x2, 0x10);
    let ys = read(0x4, 0x20);
    let mut out = vec![];
    let mut s = 0;
    for e in ends {
        out.push((s..=e).map(|i| Pt { x: xs[i] as f64, y: ys[i] as f64, on: flags[i] & 1 != 0 }).collect());
        s = e + 1;
    }
    out
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

/// Unicode → glyph
pub fn charmap(font: &FontRef) -> std::collections::BTreeMap<u32, u32> {
    use skrifa::MetadataProvider;
    font.charmap().mappings().map(|(c, g)| (c, g.to_u32())).collect()
}

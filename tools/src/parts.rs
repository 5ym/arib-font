//! 部品を組んで作る字の一覧 (data/parts.txt) を読む。書き方は parts.txt の頭に書いてある。
//!
//! 組むのは格子の上 (round.rs): 部品ごとに塗って切り抜き、縮めて細った線を元の太さ (w) に戻してから重ねる。

use crate::err::{bail, Ctx, Result};
use std::collections::BTreeMap;

/// BIZ UDゴシックの漢字の線の太さ (em 1024。一 の横線 83、川 の縦線 83〜85)
pub const STEM: f64 = 84.0;

#[derive(Clone, Debug)]
pub enum Source {
    /// BIZ UDゴシックの字
    Char(u32),
    /// 輪 (中心・外の半径・太さ)
    Ring(f64, f64, f64, f64),
    /// 塗った丸 (中心・半径)
    Disc(f64, f64, f64),
    /// 四角の枠 (外の枠・太さ)
    Frame(f64, f64, f64, f64, f64),
    /// 塗った四角
    Rect(f64, f64, f64, f64),
    /// 半角の字の並び ("5.1" など)。墨の幅で詰めて並べ、置き方の枠に縦横同じ比で収める
    Text(String),
}

#[derive(Clone, Debug)]
pub enum Place {
    /// 元の字の座標のまま
    Keep,
    /// 切り抜き (無ければ墨の枠) を枠いっぱいに伸ばす
    Stretch(f64, f64, f64, f64),
    /// 縦横同じ比で枠に収め、真ん中に置く
    Fit(f64, f64, f64, f64),
    /// 大きさはそのまま動かす
    Shift(f64, f64),
    /// 大きさはそのまま、墨の横の真ん中を x に置く (半角と全角の幅を替える字)
    CenterX(f64),
}

#[derive(Clone, Debug)]
pub struct Part {
    pub sub: bool,
    pub src: Source,
    /// 使う輪郭の番号 (無ければ全部)
    pub pick: Option<Vec<usize>>,
    /// 切り抜く多角形 (元の字の座標)
    pub crop: Option<Vec<(f64, f64)>>,
    pub place: Place,
    /// 縮めた線を戻す太さ (0 なら戻さない)
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub enum Entry {
    Parts { advance: u16, parts: Vec<Part> },
    Alias(u32),
}

fn nums(s: &str) -> Result<Vec<f64>> {
    s.split([',', ' ']).filter(|t| !t.is_empty()).map(|t| t.parse::<f64>().ctx(|| format!("数が読めません: {t}"))).collect()
}

fn codepoint(s: &str) -> Result<u32> {
    if let Some(h) = s.strip_prefix("U+") {
        return u32::from_str_radix(h, 16).ctx(|| format!("U+ が読めません: {s}"));
    }
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(c as u32),
        _ => bail!("1 字でない: {s}"),
    }
}

/// 中身を [] か () で囲んだ字句を切り出す。返すのは (中身, 残り)
fn bracket<'a>(s: &'a str, open: char, close: char) -> Option<(&'a str, &'a str)> {
    let r = s.strip_prefix(open)?;
    let e = r.find(close)?;
    Some((&r[..e], &r[e + close.len_utf8()..]))
}

fn part(text: &str) -> Result<Part> {
    let mut s = text.trim();
    let sub = if let Some(r) = s.strip_prefix('-') {
        s = r.trim_start();
        true
    } else {
        false
    };
    // 元
    let src;
    let shape = |name: &str, s: &str| -> Option<(Vec<f64>, String)> {
        let (inner, rest) = bracket(s.strip_prefix(name)?, '(', ')')?;
        Some((nums(inner).ok()?, rest.to_string()))
    };
    let rest: String;
    if let Some((v, r)) = shape("ring", s).filter(|(v, _)| v.len() == 4) {
        src = Source::Ring(v[0], v[1], v[2], v[3]);
        rest = r;
    } else if let Some((v, r)) = shape("disc", s).filter(|(v, _)| v.len() == 3) {
        src = Source::Disc(v[0], v[1], v[2]);
        rest = r;
    } else if let Some((v, r)) = shape("frame", s).filter(|(v, _)| v.len() == 5) {
        src = Source::Frame(v[0], v[1], v[2], v[3], v[4]);
        rest = r;
    } else if let Some((t, r)) = s.strip_prefix('"').and_then(|r| r.split_once('"')) {
        src = Source::Text(t.to_string());
        rest = r.to_string();
    } else if let Some((v, r)) = shape("rect", s).filter(|(v, _)| v.len() == 4) {
        src = Source::Rect(v[0], v[1], v[2], v[3]);
        rest = r;
    } else {
        let tok = s.split_whitespace().next().ctx(|| format!("部品が空: {text}"))?;
        src = Source::Char(codepoint(tok)?);
        rest = s[tok.len()..].to_string();
    }
    let mut s = rest.trim();
    // 輪郭の番号
    let mut pick = None;
    if let Some((inner, r)) = bracket(s, '{', '}') {
        pick = Some(inner.split(',').map(|t| t.trim().parse::<usize>().ctx(|| format!("輪郭の番号が読めません: {text}"))).collect::<Result<Vec<_>>>()?);
        s = r.trim();
    }
    // 切り抜き
    let mut crop = None;
    if let Some((inner, r)) = bracket(s, '[', ']') {
        let v = nums(inner)?;
        crop = Some(if v.len() == 4 {
            vec![(v[0], v[1]), (v[2], v[1]), (v[2], v[3]), (v[0], v[3])]
        } else if v.len() >= 6 && v.len() % 2 == 0 {
            v.chunks(2).map(|c| (c[0], c[1])).collect()
        } else {
            bail!("切り抜きは 4 つの数か 3 点以上: {text}")
        });
        s = r.trim();
    }
    // 置き方
    let mut place = Place::Keep;
    let mut weight = STEM;
    for tok in tokens(s) {
        if let Some(r) = tok.strip_prefix('>') {
            let v = nums(r)?;
            if v.len() != 4 {
                bail!("> の枠は 4 つの数: {text}");
            }
            place = Place::Stretch(v[0], v[1], v[2], v[3]);
        } else if let Some(r) = tok.strip_prefix('~') {
            let v = nums(r)?;
            if v.len() != 4 {
                bail!("~ の枠は 4 つの数: {text}");
            }
            place = Place::Fit(v[0], v[1], v[2], v[3]);
        } else if let Some(r) = tok.strip_prefix('+') {
            let v = nums(r)?;
            if v.len() != 2 {
                bail!("+ の動かす量は 2 つの数: {text}");
            }
            place = Place::Shift(v[0], v[1]);
        } else if let Some(r) = tok.strip_prefix("cx") {
            place = Place::CenterX(r.parse().ctx(|| format!("cx が読めません: {text}"))?);
        } else if let Some(r) = tok.strip_prefix('w') {
            weight = r.parse().ctx(|| format!("w が読めません: {text}"))?;
        } else {
            bail!("読めない字句 {tok:?}: {text}");
        }
    }
    Ok(Part { sub, src, pick, crop, place, weight })
}

/// 置き方の字句 (> と ~ のあとの数は空白を挟んでもよい)
fn tokens(s: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in s.split_whitespace() {
        let starts_num = t.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == ',');
        match out.last_mut() {
            Some(last) if starts_num && (last.starts_with('>') || last.starts_with('~')) => {
                last.push(',');
                last.push_str(t);
            }
            _ => out.push(t.to_string()),
        }
    }
    out
}

/// data/parts.txt を読む
pub fn load(path: &str) -> Result<BTreeMap<u32, Entry>> {
    let text = std::fs::read_to_string(path).ctx(|| path.to_string())?;
    parse(&text, path)
}

#[cfg(test)]
pub fn load_str(text: &str) -> Result<BTreeMap<u32, Entry>> {
    parse(text, "(test)")
}

fn parse(text: &str, path: &str) -> Result<BTreeMap<u32, Entry>> {
    let mut macros: BTreeMap<String, String> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (no, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let at = || format!("{path}:{}", no + 1);
        if let Some(r) = line.strip_prefix('@') {
            let (name, body) = r.split_once('=').ctx(|| format!("{}: @名前 = 部品 ; …", at()))?;
            macros.insert(name.trim().to_string(), body.trim().to_string());
            continue;
        }
        let (head, body) = if let Some((h, b)) = line.split_once("->") {
            let t = codepoint(b.trim()).ctx(at)?;
            (h, Err(t))
        } else {
            let (h, b) = line.split_once('=').ctx(|| format!("{}: U+XXXX 字 = 部品 ; …", at()))?;
            (h, Ok(b))
        };
        let mut ht = head.split_whitespace();
        let cp = codepoint(ht.next().ctx(at)?).ctx(at)?;
        let _glyph = ht.next(); // 字そのもの (読むため)
        let advance: u16 = match ht.next() {
            Some(a) => a.parse().ctx(at)?,
            None => 1024,
        };
        let entry = match body {
            Err(t) => Entry::Alias(t),
            Ok(b) => {
                // 型を展開する
                let mut items: Vec<String> = vec![];
                for item in b.split(';') {
                    let item = item.trim();
                    if let Some(name) = item.strip_prefix('@') {
                        let (name, extra) = name.split_once(char::is_whitespace).unwrap_or((name, ""));
                        let body = macros.get(name).ctx(|| format!("{}: 型 @{name} がありません", at()))?;
                        if !extra.trim().is_empty() {
                            bail!("{}: 型のあとには何も書けません", at());
                        }
                        items.extend(body.split(';').map(|s| s.trim().to_string()));
                    } else if !item.is_empty() {
                        items.push(item.to_string());
                    }
                }
                let parts = items.iter().map(|i| part(i)).collect::<Result<Vec<_>>>().ctx(at)?;
                Entry::Parts { advance, parts }
            }
        };
        if out.insert(cp, entry).is_some() {
            bail!("{}: U+{cp:04X} が 2 度あります", at());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_parts() {
        let t = part("\"5.1\" ~ 0,0,512,512 w60").unwrap();
        assert!(matches!(t.src, Source::Text(ref s) if s == "5.1") && matches!(t.place, Place::Fit(..)) && t.weight == 60.0);
        let p = part("林 [0,-120,470,900] > 0 -120 430 900 w70").unwrap();
        assert!(matches!(p.src, Source::Char(0x6797)));
        assert_eq!(p.crop.as_ref().unwrap().len(), 4);
        assert!(matches!(p.place, Place::Stretch(a, _, c, _) if a == 0.0 && c == 430.0));
        assert_eq!(p.weight, 70.0);
        let p = part("-ring(512,389,490,45)").unwrap();
        assert!(p.sub && matches!(p.src, Source::Ring(..)));
        let p = part("U+0028 [0,0 100,0 100,100] ~0,0,512,512").unwrap();
        assert!(matches!(p.src, Source::Char(0x28)));
        assert_eq!(p.crop.unwrap().len(), 3);
        assert!(matches!(p.place, Place::Fit(..)));
    }
}

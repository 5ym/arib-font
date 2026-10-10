//! 収める字の一覧を、denpa の表 (版を固定) から作る。フォント側に表の写しは持たない。
//!
//! 読む表 (danything/denpa の DENPA_COMMIT の版):
//!   - src/lib/ts/b24-tables.ts   字幕の文字の表 (かな・英数・半角記号・追加記号・JIS の差分・DRCS の置き換え先)
//!   - src/lib/ts/aribtext-gaiji.ts  番組表などの ARIB 外字の表 (GAIJI)
//!   - src/lib/fold.ts            検索の寄せの表 (FOLD。寄せた先の字)
//!   - package.json の web-bml の版 → npm の web-bml の jis_to_unicode_map.js (データ放送)
//! 表はいずれ 1 つのファイルにまとまる予定なので、名前 (HIRAGANA・GAIJI など) をどのファイルからでも探す。
//!
//! 入れるもの: データ放送 (web-bml の表と ASCII)、字幕 (JIS X 0208 の割り当てのある区点と外字の
//! 85・86・90〜94 区、かな・英数・記号)、DRCS の置き換え先、外字の表と寄せた先の字。
//! 抜くもの: U+EC00〜ECBB (web-bml が自前の DRCS 用フォントで描く私用領域)。
//! データ放送の外字は私用領域 (U+E0xx〜E3xx) で来るので、同じ区点の字幕・外字の表の字へつなぐ (pua)。
//!
//! TS・JS は実行せず、表のリテラルだけを読む。

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use sha2::Digest;
use std::io::Read;
use std::path::Path;

/// 表を読む denpa の版
// renovate: datasource=git-refs depName=https://github.com/danything/denpa branch=main
pub const DENPA_COMMIT: &str = "9e8c8ef1951b451bfe1683a12cf99719efc6d242";
const FILES: [&str; 3] = ["src/lib/ts/b24-tables.ts", "src/lib/ts/aribtext-gaiji.ts", "src/lib/fold.ts"];

/// 私用領域の外字を何で描くか
#[derive(Clone, Debug, PartialEq)]
pub enum Pua {
    /// 同じ区点の Unicode の字と同じ字形
    Char(u32),
    /// Unicode に字が無いもの (92区56〜85点の楽器の略記)。この文字列を 1 マスに並べる
    Text(String),
}

pub struct Repertoire {
    pub codepoints: BTreeSet<u32>,
    pub pua: BTreeMap<u32, Pua>,
}

fn get(url: &str) -> Result<Option<Vec<u8>>> {
    let mut last = None;
    for _ in 0..5 {
        match ureq::get(url).call() {
            Ok(mut r) => {
                let mut buf = vec![];
                r.body_mut().as_reader().read_to_end(&mut buf)?;
                return Ok(Some(buf));
            }
            Err(ureq::Error::StatusCode(404)) => return Ok(None),
            Err(e) => last = Some(e),
        }
    }
    bail!("取れません: {url}: {}", last.unwrap())
}

/// 取ったものは build/src/denpa-<版>/ に置いて使い回す
fn cached(dir: &Path, name: &str, url: &str) -> Result<Option<Vec<u8>>> {
    let p = dir.join(name.replace('/', "_"));
    let missing = dir.join(format!("{}.404", name.replace('/', "_")));
    if let Ok(b) = std::fs::read(&p) {
        return Ok(Some(b));
    }
    if missing.exists() {
        return Ok(None);
    }
    std::fs::create_dir_all(dir)?;
    match get(url)? {
        Some(b) => {
            let tmp = p.with_extension("part");
            std::fs::write(&tmp, &b)?;
            std::fs::rename(&tmp, &p)?;
            Ok(Some(b))
        }
        None => {
            std::fs::write(&missing, b"")?;
            Ok(None)
        }
    }
}

/// `export const NAME = <値>` の <値> の文字列 (先頭から)。どのファイルにあってもよい
fn value<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("export const {name}");
    let i = src.find(&key)?;
    let rest = &src[i + key.len()..];
    let eq = rest.find('=')?;
    Some(rest[eq + 1..].trim_start())
}

fn json_prefix(s: &str) -> Result<serde_json::Value> {
    let mut de = serde_json::Deserializer::from_str(s).into_iter::<serde_json::Value>();
    Ok(de.next().context("値がありません")??)
}

fn num(s: &str) -> Option<i64> {
    let s = s.trim();
    match s.strip_prefix("0x") {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// JS の文字列リテラル (' か ") を読む。返すのは (中身, 残り)
fn js_string(s: &str) -> Option<(String, &str)> {
    let q = s.chars().next()?;
    if q != '\'' && q != '"' {
        return None;
    }
    let mut out = String::new();
    let mut it = s[1..].char_indices();
    while let Some((i, c)) = it.next() {
        match c {
            '\\' => {
                let (_, e) = it.next()?;
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| it.next().map(|x| x.1)).collect();
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    other => out.push(other),
                }
            }
            c if c == q => return Some((out, &s[1 + i + 1..])),
            c => out.push(c),
        }
    }
    None
}

/// `new Map<…>([ [キー, 値], … ])` の組を並べる (キー・値は数か文字列)
fn map_pairs(v: &str) -> Result<Vec<(String, String)>> {
    let body = &v[v.find("([").context("Map の始まりがありません")? + 2..];
    let mut out = vec![];
    let mut s = body;
    loop {
        s = s.trim_start();
        if s.starts_with("//") {
            s = &s[s.find('\n').unwrap_or(s.len())..];
            continue;
        }
        if s.starts_with("])") || s.is_empty() {
            break;
        }
        if !s.starts_with('[') {
            bail!("Map の組が読めません: {}", &s[..s.len().min(40)]);
        }
        s = s[1..].trim_start();
        let item = |s: &mut &str| -> Result<String> {
            *s = s.trim_start();
            if let Some((t, rest)) = js_string(s) {
                *s = rest;
                Ok(t)
            } else {
                let end = s.find([',', ']']).context("Map の値の終わりがありません")?;
                let t = s[..end].trim().to_string();
                *s = &s[end..];
                Ok(t)
            }
        };
        let k = item(&mut s)?;
        s = s.trim_start().strip_prefix(',').context("Map の組に , がありません")?;
        let val = item(&mut s)?;
        s = s.trim_start().strip_prefix(']').context("Map の組の ] がありません")?;
        s = s.trim_start().strip_prefix(',').unwrap_or(s);
        out.push((k, val));
    }
    Ok(out)
}

pub fn load(cache: &Path) -> Result<Repertoire> {
    let dir = cache.join(format!("denpa-{DENPA_COMMIT}"));
    let raw = |f: &str| format!("https://raw.githubusercontent.com/danything/denpa/{DENPA_COMMIT}/{f}");
    let mut src = String::new();
    for f in FILES {
        if let Some(b) = cached(&dir, f, &raw(f))? {
            src.push_str(&String::from_utf8(b)?);
            src.push('\n');
        }
    }
    let pkg = cached(&dir, "package.json", &raw("package.json"))?.context("denpa の package.json がありません")?;
    let pkg: serde_json::Value = serde_json::from_slice(&pkg)?;
    let ver = [&pkg["dependencies"]["web-bml"], &pkg["devDependencies"]["web-bml"]]
        .into_iter()
        .find_map(|v| v.as_str())
        .context("package.json に web-bml がありません")?
        .trim_start_matches(['^', '~']);
    let tgz = cached(&dir, &format!("web-bml-{ver}.tgz"), &format!("https://registry.npmjs.org/web-bml/-/web-bml-{ver}.tgz"))?
        .context("web-bml がありません")?;
    // web-bml の tgz は denpa の bun.lock の integrity (sha512) で確かめる
    let lock = cached(&dir, "bun.lock", &raw("bun.lock"))?.context("denpa の bun.lock がありません")?;
    let lock = String::from_utf8(lock)?;
    let line = lock
        .lines()
        .find(|l| l.contains(&format!("[\"web-bml@{ver}\"")))
        .with_context(|| format!("bun.lock に web-bml@{ver} がありません"))?;
    let want = line
        .rsplit("\"sha512-")
        .next()
        .and_then(|s| s.split('"').next())
        .context("bun.lock の web-bml に integrity がありません")?;
    let got = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sha2::Sha512::digest(&tgz));
    if got != want {
        bail!("web-bml-{ver}.tgz の sha512 が denpa の bun.lock と違います (build/src の取ったものを消して取り直す)");
    }
    let mut jis = String::new();
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(&tgz[..]));
    for e in ar.entries()? {
        let mut e = e?;
        if e.path()?.ends_with("dist/client/jis_to_unicode_map.js") {
            e.read_to_string(&mut jis)?;
        }
    }
    if jis.is_empty() {
        bail!("web-bml {ver} に jis_to_unicode_map.js がありません");
    }

    let bad = |c: u32| c == 0xFFFD || c < 0x20;
    let mut out = BTreeSet::new();

    // データ放送
    let body = jis.split("exports.jisToUnicodeMap = [").nth(1).context("jisToUnicodeMap がありません")?;
    let body = body.split("];").next().unwrap();
    let mut bml: BTreeMap<usize, u32> = BTreeMap::new();
    let mut cell = 0usize;
    for line in body.lines() {
        for tok in line.split("//").next().unwrap().split(',') {
            if tok.trim().is_empty() {
                continue;
            }
            if tok.contains('[') || tok.contains(']') {
                bail!("jisToUnicodeMap に配列の要素があります (読み手を直す)");
            }
            let v = num(tok).with_context(|| format!("jisToUnicodeMap: {tok}"))?;
            if v >= 0 {
                bml.insert(cell, v as u32);
                if !bad(v as u32) {
                    out.insert(v as u32);
                }
            }
            cell += 1;
        }
    }
    out.extend(0x20..0x7F);

    // 字幕
    for name in ["HIRAGANA", "KATAKANA", "KANA_SYMBOLS_HALF", "JISX0201", "JISX0201_HALF", "ALNUM_FULL", "ALNUM_HALF", "KANJI_SYMBOLS_HALF", "ADDITIONAL"] {
        let v = json_prefix(value(&src, name).with_context(|| format!("denpa の表に {name} がありません"))?)?;
        out.extend(v.as_str().with_context(|| format!("{name} が文字列でない"))?.chars().map(|c| c as u32).filter(|&c| !bad(c)));
    }
    let mut diff: BTreeMap<usize, u32> = BTreeMap::new();
    for pair in json_prefix(value(&src, "KANJI_DIFF").context("KANJI_DIFF がありません")?)?.as_array().context("KANJI_DIFF")? {
        let start = pair[0].as_u64().context("KANJI_DIFF の番号")? as usize;
        for (k, ch) in pair[1].as_str().context("KANJI_DIFF の字")?.chars().enumerate() {
            diff.insert(start + k, ch as u32);
        }
    }
    let gaiji_row = |ku: usize| ku == 84 || ku == 85 || (89..=93).contains(&ku); // 0 始まり: 85・86・90〜94 区
    let caption = |i: usize| -> u32 {
        diff.get(&i).copied().unwrap_or_else(|| {
            let bytes = [(i / 94 + 0xA1) as u8, (i % 94 + 0xA1) as u8];
            let (s, _, _) = encoding_rs::EUC_JP.decode(&bytes);
            s.chars().next().map_or(0xFFFD, |c| c as u32)
        })
    };
    for i in 0..94 * 94 {
        if !bml.contains_key(&i) && !gaiji_row(i / 94) {
            continue; // 割り当てのない区点は入れない
        }
        let c = caption(i);
        if !bad(c) {
            out.insert(c);
        }
    }
    out.insert(0x3000);

    // DRCS の置き換え先
    for (_, v) in map_pairs(value(&src, "DRCS_REPLACE").context("DRCS_REPLACE がありません")?)? {
        out.insert(num(&v).with_context(|| format!("DRCS_REPLACE: {v}"))? as u32);
    }

    // 外字の表 (番組表など) と、検索の寄せた先
    let mut gaiji: BTreeMap<usize, String> = BTreeMap::new();
    if let Some(v) = value(&src, "GAIJI") {
        for (k, s) in map_pairs(v)? {
            let code = num(&k).with_context(|| format!("GAIJI: {k}"))? as usize;
            let (hi, lo) = (code >> 8, code & 0xff);
            if !(0x21..=0x7e).contains(&hi) || !(0x21..=0x7e).contains(&lo) {
                bail!("GAIJI のキーが区点でない: {k}");
            }
            gaiji.insert((hi - 0x21) * 94 + (lo - 0x21), s.clone());
            out.extend(s.chars().map(|c| c as u32).filter(|&c| !bad(c)));
        }
    } else {
        eprintln!("注意: denpa の表に GAIJI がありません");
    }
    if let Some(v) = value(&src, "FOLD") {
        for (_, s) in map_pairs(v)? {
            out.extend(s.chars().map(|c| c as u32).filter(|&c| !bad(c)));
        }
    }

    // データ放送の外字 (私用領域) → 同じ区点の字
    let mut pua = BTreeMap::new();
    for (&i, &p) in &bml {
        if !(0xE000..=0xF8FF).contains(&p) || (0xEC00..=0xECBB).contains(&p) {
            continue;
        }
        let c = caption(i);
        let target = if !bad(c) && !(0xE000..=0xF8FF).contains(&c) {
            Pua::Char(c)
        } else if let Some(s) = gaiji.get(&i) {
            let mut cs = s.chars();
            match (cs.next(), cs.next()) {
                (Some(one), None) => Pua::Char(one as u32),
                _ => Pua::Text(s.clone()),
            }
        } else {
            bail!("U+{p:04X} ({}区{}点) をつなぐ字が denpa の表にありません", i / 94 + 1, i % 94 + 1);
        };
        pua.insert(p, target);
    }
    let codepoints = out.into_iter().filter(|c| !(0xEC00..=0xECBB).contains(c)).collect();
    Ok(Repertoire { codepoints, pua })
}

/// 一覧を書き出す (見るため。ビルドは load を直に使う)
pub fn run(cache: &Path) -> Result<()> {
    let r = load(cache)?;
    for c in &r.codepoints {
        match r.pua.get(c) {
            Some(Pua::Char(t)) => println!("U+{c:04X}  # → U+{t:04X} {}", char::from_u32(*t).unwrap_or('?')),
            Some(Pua::Text(t)) => println!("U+{c:04X}  # → {t}"),
            None => println!("U+{c:04X}"),
        }
    }
    eprintln!("{} code points (denpa {DENPA_COMMIT})", r.codepoints.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings() {
        assert_eq!(js_string("'a\\'b' rest"), Some(("a'b".to_string(), " rest")));
        assert_eq!(js_string("\"\\u3042\""), Some(("あ".to_string(), "")));
        assert_eq!(js_string("x"), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(num(" 0x7c21"), Some(0x7c21));
        assert_eq!(num("-1"), Some(-1));
        assert_eq!(num("x"), None);
    }

    #[test]
    fn maps() {
        let src = "new Map<number, string>([\n    // 92区\n    [0x7c21, '➡'],\n    [0x7c58, '(vn)'], // 楽器\n    ['abc', 0x269e],\n]);";
        assert_eq!(
            map_pairs(src).unwrap(),
            vec![
                ("0x7c21".to_string(), "➡".to_string()),
                ("0x7c58".to_string(), "(vn)".to_string()),
                ("abc".to_string(), "0x269e".to_string()),
            ]
        );
    }

    #[test]
    fn values() {
        let src = "export const A = \"x\\\"y\";\nexport const B: [number, string][] = [[1,\"z\"]];";
        assert_eq!(json_prefix(value(src, "A").unwrap()).unwrap(), serde_json::json!("x\"y"));
        assert_eq!(json_prefix(value(src, "B").unwrap()).unwrap(), serde_json::json!([[1, "z"]]));
        assert!(value(src, "C").is_none());
    }
}

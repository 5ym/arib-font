//! 元フォントを取ってきて (配布元・sha256 を固定)、書庫から要るファイルだけ出す。

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

pub struct Source {
    pub url: &'static str,
    pub sha256: &'static str,
    pub files: &'static [&'static str],
}

/// 源柔ゴシック 1.002.20150607 (http://jikasei.me/font/genjyuu/)。等幅 Regular を元にする
pub const GENJYUU: Source = Source {
    url: "https://ftp.iij.ad.jp/pub/osdn.jp/users/8/8636/genjyuugothic-20150607.7z",
    sha256: "1997876351985ac9e2ea7a7a809c91e3fdaef878ece5b830f2512c882c3e49db",
    files: &["GenJyuuGothic-Monospace-Regular.ttf"],
};

pub fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// 書庫を取ってきて (あれば使い回す) sha256 を確かめ、要るファイルを dir に出す
pub fn fetch(src: &Source, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let name = src.url.rsplit('/').next().unwrap();
    let arc = dir.join(name);
    let ok = |p: &Path| fs::read(p).map(|b| sha256_hex(&b) == src.sha256).unwrap_or(false);
    if !ok(&arc) {
        let mut last = None;
        for _ in 0..5 {
            match ureq::get(src.url).call() {
                Ok(mut r) => {
                    let mut buf = vec![];
                    r.body_mut().as_reader().read_to_end(&mut buf)?;
                    fs::write(&arc, &buf)?;
                    last = None;
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        if let Some(e) = last {
            bail!("取れません: {}: {e}", src.url);
        }
        if !ok(&arc) {
            bail!("sha256 が違います: {}", src.url);
        }
    }
    if name.ends_with(".7z") {
        let mut r = sevenz_rust2::ArchiveReader::open(&arc, sevenz_rust2::Password::empty())
            .with_context(|| format!("7z を開けません: {name}"))?;
        r.for_each_entries(|entry, reader| {
            if src.files.contains(&entry.name()) {
                let mut buf = vec![];
                reader.read_to_end(&mut buf)?;
                fs::write(dir.join(entry.name()), buf)?;
            } else {
                std::io::copy(reader, &mut std::io::sink())?;
            }
            Ok(true)
        })?;
    } else if name.ends_with(".lzh") {
        let mut lha = delharc::parse_file(&arc).with_context(|| format!("lzh を開けません: {name}"))?;
        loop {
            let path = lha.header().parse_pathname();
            let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            if src.files.contains(&file.as_str()) {
                let mut buf = vec![];
                lha.read_to_end(&mut buf)?;
                lha.crc_check()?;
                fs::write(dir.join(&file), buf)?;
            }
            if !lha.next_file()? {
                break;
            }
        }
    } else {
        bail!("知らない書庫: {name}");
    }
    for f in src.files {
        if !dir.join(f).exists() {
            bail!("{name} に {f} がありません");
        }
    }
    Ok(())
}

//! Mod files (tier 6): fetched once from their hosts into `.cache/mods/<id>/`, verified against the
//! hash recorded in `input/config/mods.json`, and unpacked (7z, or innoextract for Inno Setup
//! installers; both external tools, like curl). The ModDB and Mediafire resolvers follow
//! `~/.nix/lib/fetchers/{moddb,mediafire}.nix`. Nexus files are downloaded by hand, as portmod does:
//! the user saves them in the downloads folder, and they are copied from there.

use log::info;
use serde::Deserialize;
use sha2::Digest;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MANIFEST: &str = "input/config/mods.json";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";
/// ModDB sits behind Cloudflare, which flags curl's TLS 1.3 handshake; TLS 1.2 passes.
const MODDB_CURL: &[&str] = &["--tls-max", "1.2", "--header", "Referer: https://www.moddb.com/"];

#[derive(Deserialize)]
struct ModEntry {
    id: String,
    source: Source,
    file: String,
    md5: Option<String>,
    sha256: Option<String>,
    /// Archive members to unpack (7z wildcards); all of it when absent.
    #[serde(default)]
    unpack: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Source {
    Moddb(u64),
    Mediafire(String),
    /// A Nexus file, downloaded by hand (Nexus serves files to free accounts only through its site).
    Nexus { game: String, #[serde(rename = "mod")] mod_id: u64, file: u64 },
}

/// Where hand-downloaded files are looked for: the XDG downloads folder, else `~/Downloads`.
fn downloads_dir() -> PathBuf {
    std::env::var_os("XDG_DOWNLOAD_DIR").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Downloads"))
}

fn curl(extra: &[&str], url: &str) -> Command {
    let mut cmd = Command::new("curl");
    cmd.args(["-sSL", "--fail", "--user-agent", USER_AGENT]).args(extra).arg(url);
    cmd
}

fn page(extra: &[&str], url: &str) -> Result<String, String> {
    let out = curl(extra, url).output().map_err(|e| format!("curl: {}", e))?;
    if !out.status.success() { return Err(format!("{} failed: {}", url, String::from_utf8_lossy(&out.stderr).trim())); }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The mirror link on ModDB's download-start page.
fn moddb_mirror(start_page: &str) -> Option<String> {
    let at = start_page.find("href=\"/downloads/mirror/")? + "href=\"".len();
    let end = start_page[at..].find('"')?;
    Some(format!("https://www.moddb.com{}", &start_page[at..at + end]))
}

/// The CDN link on a Mediafire file page (any download*.mediafire.com URL).
fn mediafire_direct(file_page: &str) -> Option<String> {
    let mut rest = file_page;
    while let Some(i) = rest.find("://download") {
        let start = rest[..i].rfind("http").unwrap_or(i);
        let url_end = rest[i..].find(|c: char| matches!(c, '"' | '\'' | '<' | '>' | ' ')).map_or(rest.len(), |e| i + e);
        let url = &rest[start..url_end];
        if url.contains(".mediafire.com/") { return Some(url.to_string()); }
        rest = &rest[url_end..];
    }
    None
}

fn hex_digest<D: Digest>(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let (mut hasher, mut buf) = (D::new(), vec![0u8; 1 << 20]);
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The unpacked files of mod `id` (fetching and unpacking them first if needed).
pub fn fetch(id: &str) -> Result<PathBuf, String> {
    let manifest: Vec<ModEntry> = serde_json::from_slice(&std::fs::read(MANIFEST).map_err(|e| format!("{}: {}", MANIFEST, e))?)
        .map_err(|e| format!("{}: {}", MANIFEST, e))?;
    let m = manifest.iter().find(|m| m.id == id).ok_or_else(|| format!("{} is not in {}", id, MANIFEST))?;
    let dir = Path::new(".cache/mods").join(id);
    let (archive, unpacked) = (dir.join(&m.file), dir.join("files"));
    if unpacked.is_dir() { return Ok(unpacked); }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    if let (Source::Nexus { game, mod_id, file }, false) = (&m.source, archive.exists()) {
        let downloaded = downloads_dir().join(&m.file);
        if !downloaded.is_file() {
            return Err(format!("download https://www.nexusmods.com/{}/mods/{}?tab=files&file_id={} by hand into {} (as {})",
                game, mod_id, file, downloads_dir().display(), m.file));
        }
        std::fs::copy(&downloaded, &archive).map_err(|e| format!("copying {}: {}", downloaded.display(), e))?;
    }
    if !archive.exists() {
        let (url, extra): (String, &[&str]) = match &m.source {
            Source::Moddb(n) => {
                let start = page(MODDB_CURL, &format!("https://www.moddb.com/downloads/start/{}/all", n))?;
                (moddb_mirror(&start).ok_or("no mirror link on ModDB's download page (bot check or layout change)")?, MODDB_CURL)
            }
            Source::Mediafire(url) => (mediafire_direct(&page(&[], url)?).unwrap_or_else(|| url.clone()), &[]),
            Source::Nexus { .. } => unreachable!("copied above"),
        };
        info!("[Mods] downloading {} ({})", m.file, id);
        let part = dir.join(format!("{}.part", m.file));
        let status = curl(extra, &url).arg("-o").arg(&part).status().map_err(|e| format!("curl: {}", e))?;
        if !status.success() { return Err(format!("download of {} failed", m.file)); }
        std::fs::rename(&part, &archive).map_err(|e| e.to_string())?;
    }

    let (expected, actual) = match (&m.sha256, &m.md5) {
        (Some(h), _) => (h, hex_digest::<sha2::Sha256>(&archive)?),
        (None, Some(h)) => (h, hex_digest::<md5::Md5>(&archive)?),
        (None, None) => return Err(format!("{} has no hash in {}: record its sha256, {}, once the file is checked",
            id, MANIFEST, hex_digest::<sha2::Sha256>(&archive)?)),
    };
    if !actual.eq_ignore_ascii_case(expected) {
        let _ = std::fs::remove_file(&archive); // fetched (or copied) again next time
        return Err(format!("{} does not match its recorded hash ({} != {})", m.file, actual, expected));
    }

    // 7z reads zip/7z/rar and NSIS installers; innoextract reads Inno Setup installers (Nevada's).
    info!("[Mods] unpacking {}", m.file);
    let quiet = || std::process::Stdio::null();
    let sevenzip = Command::new("7z").arg("x").arg("-y").arg(format!("-o{}", unpacked.display())).arg(&archive)
        .args(&m.unpack).stdout(quiet()).stderr(quiet()).status().is_ok_and(|s| s.success());
    if !sevenzip {
        let _ = std::fs::remove_dir_all(&unpacked);
        let inno = Command::new("innoextract").arg("--silent").arg("--output-dir").arg(&unpacked).arg(&archive)
            .stdout(quiet()).status().is_ok_and(|s| s.success());
        if !inno {
            let _ = std::fs::remove_dir_all(&unpacked);
            return Err(format!("unpacking {} failed (neither 7z nor innoextract can read it)", m.file));
        }
    }
    Ok(unpacked)
}

/// The first directory under `root` (breadth-first, `root` included) holding a file named `name`
/// (case-insensitive).
pub fn find_dir_with(root: &Path, name: &str) -> Option<PathBuf> {
    let mut queue = vec![root.to_path_buf()];
    while !queue.is_empty() {
        let mut next = Vec::new();
        for dir in queue {
            let entries: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
            if entries.iter().any(|p| p.is_file() && p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(name))) { return Some(dir); }
            next.extend(entries.into_iter().filter(|p| p.is_dir()));
        }
        queue = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses() {
        let manifest: Vec<ModEntry> = serde_json::from_slice(&std::fs::read(MANIFEST).unwrap()).unwrap();
        assert!(!manifest.is_empty());
        let nexus: Source = serde_json::from_str(r#"{ "nexus": { "game": "fallout3", "mod": 1, "file": 2 } }"#).unwrap();
        assert!(matches!(nexus, Source::Nexus { mod_id: 1, file: 2, .. }));
    }

    #[test]
    fn link_parsers() {
        let moddb = r#"<a href="/downloads/mirror/298750/130/f4cf59e1dfb694081d95b0cf0c9f92bc" class="buttondownload">"#;
        assert_eq!(moddb_mirror(moddb).as_deref(), Some("https://www.moddb.com/downloads/mirror/298750/130/f4cf59e1dfb694081d95b0cf0c9f92bc"));
        let mf = r#"<a class="input popsok" aria-label="Download file" href="https://download2390.mediafire.com/abc/nqf/Fallout_Nevada_Mod_Us.exe" id="downloadButton">"#;
        assert_eq!(mediafire_direct(mf).as_deref(), Some("https://download2390.mediafire.com/abc/nqf/Fallout_Nevada_Mod_Us.exe"));
        assert_eq!(mediafire_direct("<html>no link</html>"), None);
    }
}

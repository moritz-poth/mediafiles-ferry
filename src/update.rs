//! Updates from GitHub releases (port of Mountpoint's Updater.swift; the same file is in MediaFiles Check).
//! Newest non-draft release with the zip asset → download → unzip with ditto → check bundle id, release key ==
//! tag and the signature → old app to the Trash, new app in its place → restart once this process has quit.
//! Network through /usr/bin/curl (part of macOS), so the app needs no TLS library.
use serde::Deserialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What this app is on GitHub and in its Info.plist.
pub struct App {
    /// "moritz-poth/mediafiles-check"
    pub repo: &'static str,
    /// app bundle name without ".app", also the zip prefix with "-" for spaces: "MediaFiles Check" → "MediaFiles-Check-<version>.zip"
    pub name: &'static str,
    pub bundle_id: &'static str,
    /// Info.plist key holding the full release version (e.g. "1.0.0-beta.2"), set by the build script
    pub release_key: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub tag: String,
    /// release page ("What's new")
    pub page: String,
    pub zip: String,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// GitHub not reachable or no readable answer
    Check,
    Download,
    /// the zip or the app in it is not the expected, intact version
    Damaged,
    /// the running app could not be replaced (e.g. not writable, other volume); details from the system
    Replace(String),
    /// installed, but the new version could not be started – quit and open again
    Restart,
}

/// Version from a tag like "v1.0.0-beta.7": numbers and beta number (None = final release, ranks above its betas).
pub fn version(text: &str) -> Option<(Vec<u32>, Option<u32>)> {
    let s = text.trim().trim_start_matches('v');
    let (nums, beta) = match s.split_once('-') {
        Some((n, b)) => (n, Some(b.to_lowercase().strip_prefix("beta.")?.parse().ok()?)),
        None => (s, None),
    };
    let nums: Vec<u32> = nums.split('.').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    Some((nums, beta))
}

/// Compares two version texts; None if one of them is not a version.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    let ((mut x, p), (mut y, q)) = (version(a)?, version(b)?);
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    Some(x.cmp(&y).then(match (p, q) {
        (Some(p), Some(q)) => p.cmp(&q),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }))
}

/// Newest non-draft release that has the app's zip, from GitHub's releases JSON.
pub fn newest_release(json: &[u8], app: &App) -> Option<Release> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct R {
        tag_name: String,
        html_url: String,
        draft: bool,
        assets: Vec<Asset>,
    }
    let prefix = format!("{}-", app.name.replace(' ', "-"));
    let list: Vec<R> = serde_json::from_slice(json).ok()?;
    list.into_iter()
        .filter(|r| !r.draft && version(&r.tag_name).is_some())
        .filter_map(|r| {
            let zip = r.assets.into_iter().find(|a| a.name.starts_with(&prefix) && a.name.ends_with(".zip"))?;
            Some(Release { tag: r.tag_name, page: r.html_url, zip: zip.browser_download_url })
        })
        .max_by(|a, b| compare(&a.tag, &b.tag).unwrap_or(Ordering::Equal))
}

fn quiet(cmd: &mut Command) -> bool {
    cmd.stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// The newest release if it is newer than `current` (the running version), None if up to date.
pub fn check(app: &App, current: &str) -> Result<Option<Release>, Error> {
    let url = format!("https://api.github.com/repos/{}/releases?per_page=20", app.repo);
    let out = Command::new("/usr/bin/curl")
        .args(["-fsSL", "--max-time", "30", "-H", "Accept: application/vnd.github+json", &url])
        .stderr(Stdio::null())
        .output()
        .map_err(|_| Error::Check)?;
    if !out.status.success() {
        return Err(Error::Check);
    }
    let release = newest_release(&out.stdout, app).ok_or(Error::Check)?;
    Ok((compare(current, &release.tag) == Some(Ordering::Less)).then_some(release))
}

fn plist_value(app: &Path, key: &str) -> Option<String> {
    let out = Command::new("/usr/bin/plutil").args(["-extract", key, "raw", "-o", "-"]).arg(app.join("Contents/Info.plist")).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Unzips into a fresh temporary folder and checks that it really is the expected, intact version of this app.
pub fn unpack(zip: &Path, tag: &str, app: &App) -> Result<PathBuf, Error> {
    let dir = std::env::temp_dir().join(format!("{}-update-{}-{}", app.bundle_id, std::process::id(), stamp()));
    std::fs::create_dir_all(&dir).map_err(|_| Error::Damaged)?;
    if !quiet(Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(zip).arg(&dir)) {
        return Err(Error::Damaged);
    }
    let new = dir.join(format!("{}.app", app.name));
    let intact = plist_value(&new, "CFBundleIdentifier").as_deref() == Some(app.bundle_id)
        && plist_value(&new, app.release_key).is_some_and(|r| compare(&r, tag) == Some(Ordering::Equal))
        && quiet(Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&new));
    if intact { Ok(new) } else { Err(Error::Damaged) }
}

/// Nanoseconds since 1970 – keeps temporary files and trashed versions apart.
fn stamp() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}

/// Downloads the release's zip and unpacks it (see `unpack`). Runs for a while – call it off the UI thread.
pub fn download(release: &Release, app: &App) -> Result<PathBuf, Error> {
    let zip = std::env::temp_dir().join(format!("{}-{}.zip", app.bundle_id, stamp()));
    let ok = quiet(Command::new("/usr/bin/curl").args(["-fsSL", "--max-time", "900", "-o"]).arg(&zip).arg(&release.zip));
    let result = if ok { unpack(&zip, &release.tag, app) } else { Err(Error::Download) };
    let _ = std::fs::remove_file(&zip);
    result
}

/// The .app bundle this process runs from.
pub fn running_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

/// Puts `new` in place of the running app (the old one goes to the Trash, restored if the move fails) and starts
/// a shell that opens it once this process has quit. The caller quits right after Ok.
pub fn swap_and_restart(new: &Path) -> Result<(), Error> {
    let current = running_bundle().ok_or_else(|| Error::Replace("not started from an app bundle".into()))?;
    let trash = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".Trash")).ok_or_else(|| Error::Replace("no home folder".into()))?;
    let name = current.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut trashed = trash.join(format!("{name}.app"));
    if trashed.exists() {
        trashed = trash.join(format!("{name} {}.app", stamp()));
    }
    std::fs::rename(&current, &trashed).map_err(|e| Error::Replace(e.to_string()))?;
    let moved = std::fs::rename(new, &current).or_else(|_| {
        // other volume than the temporary folder: copy, keeping the signature intact
        if quiet(Command::new("/usr/bin/ditto").arg(new).arg(&current)) { Ok(()) } else { Err(std::io::Error::other("ditto failed")) }
    });
    if let Err(e) = moved {
        let _ = std::fs::remove_dir_all(&current);
        let _ = std::fs::rename(&trashed, &current);
        return Err(Error::Replace(e.to_string()));
    }
    let script = "while /bin/kill -0 $1 2>/dev/null; do sleep 0.2; done; /usr/bin/open \"$0\"";
    Command::new("/bin/sh")
        .args(["-c", script])
        .arg(&current)
        .arg(std::process::id().to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| Error::Restart)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: App = App { repo: "moritz-poth/x", name: "My App", bundle_id: "io.example.x", release_key: "MyAppRelease" };

    #[test]
    fn version_order() {
        let lt = |a, b| compare(a, b) == Some(Ordering::Less);
        assert!(lt("v1.0.0-beta.7", "v1.0.0-beta.10"));
        assert!(lt("1.0.0-beta.7", "1.0.0"));
        assert!(lt("1.0.0", "1.0.1-beta.1"));
        assert_eq!(compare("1.0", "1.0.0"), Some(Ordering::Equal));
        assert_eq!(compare("1.0.0-beta.7", "v1.0.0-beta.7"), Some(Ordering::Equal));
        assert_eq!(version("dev"), None);
        assert_eq!(version("1.0.0-rc"), None);
    }

    #[test]
    fn newest_release_skips_drafts_and_releases_without_zip() {
        let json = br#"[{"tag_name":"v1.0.0-beta.9","html_url":"https://x/9","draft":true,"assets":[{"name":"My-App-1.0.0-beta.9.zip","browser_download_url":"https://x/9.zip"}]},
            {"tag_name":"v1.0.0-beta.8","html_url":"https://x/8","draft":false,"assets":[]},
            {"tag_name":"v1.0.0-beta.10","html_url":"https://x/10","draft":false,"assets":[{"name":"My-App-1.0.0-beta.10.zip","browser_download_url":"https://x/10.zip"}]},
            {"tag_name":"v1.0.0-beta.7","html_url":"https://x/7","draft":false,"assets":[{"name":"My-App-1.0.0-beta.7.zip","browser_download_url":"https://x/7.zip"}]}]"#;
        let r = newest_release(json, &APP).unwrap();
        assert_eq!((r.tag.as_str(), r.zip.as_str()), ("v1.0.0-beta.10", "https://x/10.zip"));
        assert_eq!(newest_release(br#"{"message":"rate limit"}"#, &APP), None);
    }

    /// Uses the zip from scripts/build-mac-app.sh, if there is one: only the expected, intact app is accepted.
    #[test]
    fn unpack_accepts_only_the_expected_intact_app() {
        let app = &crate::UPDATE_APP;
        let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
        let prefix = format!("{}-", app.name.replace(' ', "-"));
        let Some(zip) = std::fs::read_dir(&dist).ok().and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix) && n.to_string_lossy().ends_with(".zip")))) else { return };
        let name = zip.file_name().unwrap().to_string_lossy().into_owned();
        let tag = format!("v{}", &name[prefix.len()..name.len() - 4]);
        let new = unpack(&zip, &tag, app).unwrap();
        assert!(new.join("Contents/Info.plist").exists());
        assert_eq!(unpack(&zip, "v9.9.9", app), Err(Error::Damaged));
        let other = App { bundle_id: "other.app", ..crate::UPDATE_APP };
        assert_eq!(unpack(&zip, &tag, &other), Err(Error::Damaged));
        // a changed file breaks the signature
        std::fs::write(new.join("Contents/Resources/LICENSE.txt"), "x").unwrap();
        let tampered = new.parent().unwrap().join("t.zip");
        assert!(quiet(Command::new("/usr/bin/ditto").args(["-c", "-k", "--keepParent"]).arg(&new).arg(&tampered)));
        assert_eq!(unpack(&tampered, &tag, app), Err(Error::Damaged));
        let _ = std::fs::remove_dir_all(new.parent().unwrap());
    }

    /// Live GitHub, only with UPDATE_IT=1: the published release is found and counts as newer than an older version.
    #[test]
    fn live_github_has_a_release() {
        if std::env::var("UPDATE_IT").as_deref() != Ok("1") {
            return;
        }
        let r = check(&crate::UPDATE_APP, "0.0.1").unwrap().unwrap();
        assert!(version(&r.tag).is_some() && r.zip.ends_with(".zip"));
        assert_eq!(check(&crate::UPDATE_APP, "99.0.0"), Ok(None));
    }
}

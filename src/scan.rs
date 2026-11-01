//! Laufwerke finden und MXF-Ordner mit Prüfsummen erfassen.
use crate::*;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
struct Cached {
    size: u64,
    mtime: u128,
    hash: String,
}

/// "Ordner/Datei" -> zuletzt berechneter Hash
type Cache = HashMap<String, Cached>;

pub struct ScanResult {
    pub manifest: Manifest,
    pub warnings: Vec<String>,
}

/// Alle eingehängten Laufwerke (auch Netzlaufwerke) mit "Avid MediaFiles/MXF".
pub fn find_volumes() -> Vec<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        (b'A'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))).collect()
    } else {
        fs::read_dir("/Volumes").map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default()
    };
    let mut found: Vec<PathBuf> = candidates.into_iter().filter(|p| mxf_dir(p).is_dir()).collect();
    found.sort();
    found
}

/// Ablage für Prüfsummen-Cache, Einstellungen und letztes Manifest.
pub fn app_dir() -> Option<PathBuf> {
    let (var, sub) = if cfg!(windows) {
        ("APPDATA", "MediaFiles Ferry")
    } else {
        ("HOME", "Library/Application Support/MediaFiles Ferry")
    };
    std::env::var_os(var).map(|base| PathBuf::from(base).join(sub))
}

/// Der Name, den auch Avid für Ordner wie "Mac Studio von Alex.1" nimmt; der Hostname enthält oft das Netzwerk (".fritz.box").
pub fn computer_name() -> String {
    let mac = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output().ok();
    match mac.filter(|o| o.status.success()) {
        Some(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        None => gethostname::gethostname().to_string_lossy().split('.').next().unwrap_or_default().to_string(),
    }
}

/// shortcut: unter Windows ist das der Laufwerksbuchstabe; Volume-Label per GetVolumeInformation in Phase 4.
pub fn volume_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn save_cache(path: &Path, cache: &Cache) -> io::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    write_atomic(path, &serde_json::to_vec(cache)?)
}

fn entries(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let with_path = |e: io::Error| io::Error::new(e.kind(), format!("{}: {e}", dir.display()));
    let mut v = fs::read_dir(dir).map_err(with_path)?.map(|e| e.map(|e| e.path())).collect::<io::Result<Vec<_>>>().map_err(with_path)?;
    v.sort();
    Ok(v)
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Erfasst alle MXF-Dateien. Ohne `full` wird nur neu gehasht, was sich laut Größe/Änderungsdatum verändert hat;
/// mit `full` wird alles gehasht und stille Veränderungen (Beschädigung) werden gemeldet.
pub fn scan(volumes: &[PathBuf], full: bool, cache_dir: Option<&Path>, progress: &mut dyn FnMut(&str)) -> io::Result<ScanResult> {
    let mut manifest = Manifest {
        format: FORMAT.into(),
        version: 1,
        system: computer_name(),
        created: now_secs(),
        volumes: vec![],
    };
    let mut warnings = vec![];

    for vol in volumes {
        let name = volume_name(vol);
        let cache_path = cache_dir.map(|d| d.join(format!("{name}.json")));
        let mut cache: Cache = cache_path
            .as_ref()
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let mut last_save = std::time::Instant::now();
        let mut folders = vec![];

        for dir in entries(&mxf_dir(vol))? {
            let folder_name = file_name(&dir);
            if !dir.is_dir() || folder_name.starts_with('.') {
                continue;
            }
            progress(&format!("{name}/{folder_name}"));
            let mut folder = Folder { name: folder_name.clone(), has_db: false, files: vec![] };
            let mut db = [false; 2];

            for path in entries(&dir)? {
                let n = file_name(&path);
                if n.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    if !n.eq_ignore_ascii_case("Creating") {
                        warnings.push(format!("{name}/{folder_name}/{n}: unerwarteter Unterordner, ignoriert"));
                    }
                    continue;
                }
                if let Some(i) = DB_FILES.iter().position(|d| d.eq_ignore_ascii_case(&n)) {
                    db[i] = true;
                    continue;
                }
                if !n.to_lowercase().ends_with(".mxf") {
                    continue;
                }
                let rel = format!("{folder_name}/{n}");
                let meta = match fs::metadata(&path) {
                    Ok(m) => m,
                    Err(e) => {
                        warnings.push(format!("{name}/{rel}: nicht lesbar ({e})"));
                        continue;
                    }
                };
                let size = meta.len();
                let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                let cached = cache.get(&rel).filter(|c| c.size == size && c.mtime == mtime);
                let hash = match cached {
                    Some(c) if !full => c.hash.clone(),
                    _ => match hash_file(&path) {
                        Ok(h) => {
                            if cached.is_some_and(|c| c.hash != h) {
                                warnings.push(format!("{name}/{rel}: Inhalt hat sich ohne neues Änderungsdatum verändert – Datei vermutlich beschädigt"));
                            }
                            h
                        }
                        Err(e) => {
                            warnings.push(format!("{name}/{rel}: nicht lesbar ({e})"));
                            continue;
                        }
                    },
                };
                cache.insert(rel.clone(), Cached { size, mtime, hash: hash.clone() });
                seen.insert(rel);
                folder.files.push(FileEntry { name: n, size, hash });
            }
            folder.has_db = db[0] && db[1];
            folders.push(folder);
            // Zwischenstand sichern: nach einem Absturz muss nur der Rest neu gehasht werden.
            if let Some(p) = &cache_path {
                if last_save.elapsed().as_secs() >= 30 {
                    save_cache(p, &cache)?;
                    last_save = std::time::Instant::now();
                }
            }
        }

        if let Some(p) = &cache_path {
            cache.retain(|k, _| seen.contains(k));
            save_cache(p, &cache)?;
        }
        manifest.volumes.push(Volume { name, path: vol.display().to_string(), folders });
    }
    Ok(ScanResult { manifest, warnings })
}

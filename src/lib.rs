//! MediaFiles Ferry: gleicht Avid-MXF-Bestände zweier Systeme über Manifest-Dateien ab.
pub mod collect;
pub mod plan;
pub mod request;
pub mod scan;
pub mod update;

/// This app on GitHub and in its Info.plist (the key is written by scripts/build-mac-app.sh).
pub const UPDATE_APP: update::App = update::App {
    repo: "moritz-poth/mediafiles-ferry",
    name: "MediaFiles Ferry",
    bundle_id: "io.github.moritz-poth.mediafiles-ferry",
    release_key: "MediaFilesFerryRelease",
};

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_normalization::UnicodeNormalization;

pub const FORMAT: &str = "mediafiles-ferry-manifest";
pub const DB_FILES: [&str; 2] = ["msmFMID.pmr", "msmMMOB.mdb"];

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub system: String,
    pub created: u64,
    pub volumes: Vec<Volume>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Volume {
    pub name: String,
    /// Einhängepunkt auf dem System, das das Manifest geschrieben hat
    pub path: String,
    pub folders: Vec<Folder>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Folder {
    pub name: String,
    /// beide Datenbankdateien vorhanden
    pub has_db: bool,
    pub files: Vec<FileEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub hash: String,
}

impl Manifest {
    pub fn load(path: &Path) -> io::Result<Self> {
        let m: Manifest = serde_json::from_slice(&fs::read(path)?)?;
        if m.format != FORMAT {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} is not a Ferry manifest", path.display())));
        }
        Ok(m)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        write_atomic(path, &serde_json::to_vec(self)?)
    }
}

/// Vergleichsschlüssel: macOS speichert Umlaute zerlegt (NFD), Windows zusammengesetzt (NFC) und ignoriert Groß-/Kleinschreibung.
pub fn key(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

pub fn mxf_dir(volume: &Path) -> PathBuf {
    volume.join("Avid MediaFiles").join("MXF")
}

/// Schreibt erst in eine Nachbardatei und benennt dann um: ein Absturz hinterlässt nie eine halbe Datei.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_file_name(format!("{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
    let mut f = File::create(&tmp)?;
    io::Write::write_all(&mut f, data)?;
    f.sync_all()?;
    fs::rename(&tmp, path)
}

pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut h = blake3::Hasher::new();
    h.update_reader(File::open(path)?)?;
    Ok(h.finalize().to_hex().to_string())
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub const MANIFEST_EXT: &str = "ferry";

/// Zeitpunkt in Ortszeit, z. B. Format "%y%m%d" -> "261031".
pub fn local_time(secs: u64, fmt: &str) -> String {
    chrono::DateTime::from_timestamp(secs as i64, 0).map(|t| t.with_timezone(&chrono::Local).format(fmt).to_string()).unwrap_or_default()
}

/// Datum als JJMMTT für eindeutige Ordnernamen.
pub fn yymmdd(secs: u64) -> String {
    local_time(secs, "%y%m%d")
}

/// Ersetzt Zeichen, die in Datei- und Ordnernamen auf Mac oder Windows nicht erlaubt sind.
pub fn safe_name(s: &str) -> String {
    s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

/// Dateiname eines Manifests, z. B. "261101-1312_Mac Studio_Manifest.ferry" (Inhalt ist JSON).
pub fn manifest_file_name(m: &Manifest) -> String {
    format!("{}_{}_Manifest.{MANIFEST_EXT}", local_time(m.created, "%y%m%d-%H%M"), safe_name(&m.system))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_and_key() {
        assert_eq!(yymmdd(1793448000), "261031");
        assert_eq!(yymmdd(951825600), "000229");
        assert_eq!(key("A\u{0308}vid8.1"), key("ävid8.1"));
    }
}

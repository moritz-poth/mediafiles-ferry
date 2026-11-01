//! MediaFiles Ferry: gleicht Avid-MXF-Bestände zweier Systeme über Manifest-Dateien ab.
pub mod collect;
pub mod plan;
pub mod scan;

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
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} ist kein Ferry-Manifest", path.display())));
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

/// Datum als JJMMTT (z. B. 261031) für eindeutige Ordnernamen.
/// shortcut: rechnet in UTC, kurz nach Mitternacht kann es der Vortag sein; lokale Zeitzone falls das stört.
pub fn yymmdd(secs: u64) -> String {
    // Tage seit 1970 -> Kalenderdatum (Algorithmus von H. Hinnant)
    let z = (secs / 86400) as i64 + 719468;
    let (era, doe) = (z.div_euclid(146097), z.rem_euclid(146097));
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{:02}{:02}{:02}", y % 100, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_and_key() {
        assert_eq!(yymmdd(1793448000), "261031");
        assert_eq!(yymmdd(951782400), "000229");
        assert_eq!(key("A\u{0308}vid8.1"), key("ävid8.1"));
    }
}

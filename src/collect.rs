//! Fehlende Dateien in einen Lieferordner sammeln und Lieferungen prüfen.
use crate::plan::Plan;
use crate::*;
use std::io::{Read, Write};

pub const DELIVERY_FORMAT: &str = "mediafiles-ferry-delivery";
pub const DELIVERY_FILE: &str = "ferry-lieferung.json";
pub const TASKS_FILE: &str = "Aufgaben.txt";

#[derive(Serialize, Deserialize)]
pub struct Delivery {
    pub format: String,
    pub from: String,
    pub to: String,
    pub created: u64,
    pub files: Vec<Delivered>,
    pub tasks: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Delivered {
    /// relativ zum Lieferordner: Volume/Ordner/Datei
    pub path: String,
    pub size: u64,
    pub hash: String,
}

/// Kopiert alle Dateien des Plans nach `dest/<Volume>/<Ordner>/`. Bricht bei einzelnen Fehlern nicht ab, sondern gibt sie zurück.
pub fn collect(local: &Manifest, to_system: &str, plan: &Plan, dest: &Path, progress: &mut dyn FnMut(&str)) -> io::Result<Vec<String>> {
    let mut files = vec![];
    let mut errors = vec![];
    for (i, c) in plan.copies.iter().enumerate() {
        let rel = format!("{}/{}/{}", c.to_volume, c.to_folder, c.name);
        progress(&format!("[{}/{}] {rel}", i + 1, plan.copies.len()));
        let Some(vol) = local.volumes.iter().find(|v| v.name == c.from_volume) else {
            errors.push(format!("{rel}: Volume {} fehlt im Manifest", c.from_volume));
            continue;
        };
        let src = mxf_dir(Path::new(&vol.path)).join(&c.from_folder).join(&c.name);
        let dst = dest.join(&c.to_volume).join(&c.to_folder).join(&c.name);
        match copy_verified(&src, &dst, c.hash.as_deref()) {
            Ok((size, hash)) => files.push(Delivered { path: rel, size, hash }),
            Err(e) => errors.push(format!("{rel}: {e}")),
        }
    }

    fs::create_dir_all(dest)?;
    let tasks: Vec<String> = plan.tasks.iter().map(|t| t.to_string()).collect();
    let mut text = format!(
        "MediaFiles Ferry – Lieferung von {} für {to_system}\n\n\
         Einspielen: den Inhalt jedes Ordners in den gleichnamigen Ordner unter\n\
         \"<Volume>/Avid MediaFiles/MXF/\" kopieren. Nicht den ganzen Ordner auf einen\n\
         bestehenden Ordner ziehen – der Finder ersetzt ihn sonst samt Inhalt!\n\
         Vorher in Ferry \"Lieferung prüfen\" ausführen.\n\nAufgaben:\n",
        local.system
    );
    for t in &tasks {
        text += &format!("[ ] {t}\n");
    }
    for e in &errors {
        text += &format!("[ ] Nicht kopiert: {e}\n");
    }
    fs::write(dest.join(TASKS_FILE), text)?;
    let delivery = Delivery {
        format: DELIVERY_FORMAT.into(),
        from: local.system.clone(),
        to: to_system.into(),
        created: now_secs(),
        files,
        tasks,
    };
    fs::write(dest.join(DELIVERY_FILE), serde_json::to_vec_pretty(&delivery)?)?;
    Ok(errors)
}

/// Prüft eine Lieferung gegen ihr Manifest. Gibt die gefundenen Probleme zurück (leer = alles in Ordnung).
pub fn verify(dest: &Path, progress: &mut dyn FnMut(&str)) -> io::Result<Vec<String>> {
    let d: Delivery = serde_json::from_slice(&fs::read(dest.join(DELIVERY_FILE))?)?;
    if d.format != DELIVERY_FORMAT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "keine Ferry-Lieferung"));
    }
    let mut problems = vec![];
    for (i, f) in d.files.iter().enumerate() {
        progress(&format!("[{}/{}] {}", i + 1, d.files.len(), f.path));
        match hash_file(&dest.join(&f.path)) {
            Ok(h) if h == f.hash => {}
            Ok(_) => problems.push(format!("{}: beschädigt (Prüfsumme stimmt nicht)", f.path)),
            Err(e) => problems.push(format!("{}: fehlt oder nicht lesbar ({e})", f.path)),
        }
    }
    Ok(problems)
}

/// Kopiert über eine Teildatei, prüft Quelle (gegen den Scan) und Ziel (nachgelesen) per Hash und überschreibt nie.
fn copy_verified(src: &Path, dst: &Path, expected: Option<&str>) -> io::Result<(u64, String)> {
    if dst.exists() {
        // Wiederaufnahme: gleiche Datei schon da -> überspringen
        let have = hash_file(dst)?;
        let want = match expected {
            Some(e) => e.to_string(),
            None => hash_file(src)?,
        };
        if have != want {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "existiert im Lieferordner bereits mit anderem Inhalt"));
        }
        return Ok((fs::metadata(dst)?.len(), have));
    }
    fs::create_dir_all(dst.parent().unwrap())?;
    let part = dst.with_file_name(format!("{}.ferry-part", dst.file_name().unwrap().to_string_lossy()));
    let mut hasher = blake3::Hasher::new();
    let mut input = File::open(src)?;
    let mut out = File::create(&part)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0;
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                drop(out);
                let _ = fs::remove_file(&part);
                return Err(e);
            }
        };
        hasher.update(&buf[..n]);
        if let Err(e) = out.write_all(&buf[..n]) {
            drop(out);
            let _ = fs::remove_file(&part);
            return Err(e);
        }
        size += n as u64;
    }
    out.sync_all()?;
    drop(out);

    let hash = hasher.finalize().to_hex().to_string();
    let fail = |msg: &str| {
        let _ = fs::remove_file(&part);
        Err(io::Error::other(msg.to_string()))
    };
    if expected.is_some_and(|e| e != hash) {
        return fail("Quelle weicht vom Scan ab (verändert oder beschädigt) – bitte neu scannen");
    }
    // shortcut: das Nachlesen kann aus dem OS-Cache kommen; F_NOCACHE/FILE_FLAG_NO_BUFFERING, falls echte Medienprüfung nötig wird
    if hash_file(&part)? != hash {
        return fail("Kopie fehlerhaft – Ziellaufwerk prüfen");
    }
    fs::rename(&part, dst)?;
    Ok((size, hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{self, Options};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ferry-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn scan_collect_verify_end_to_end() {
        let root = tmp("e2e");
        let vol = root.join("V");
        let folder = mxf_dir(&vol).join("Avid8.1");
        fs::create_dir_all(folder.join("Creating/creating1")).unwrap();
        for (n, c) in [("a.mxf", "aaa"), ("b.MXF", "bbb"), ("msmFMID.pmr", "db1"), ("msmMMOB.mdb", "db2"), ("notes.txt", "x")] {
            fs::write(folder.join(n), c).unwrap();
        }
        let cache = root.join("cache");
        let s = scan::scan(&[vol.clone()], false, Some(&cache), &mut |_| {}).unwrap();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        let f = &s.manifest.volumes[0].folders[0];
        assert_eq!((f.files.len(), f.has_db), (2, true));

        let remote = Manifest { system: "B".into(), volumes: vec![Volume { name: "V".into(), ..Default::default() }], ..Default::default() };
        let p = plan::plan(&s.manifest, &remote, &Options::default(), "261031");
        let dest = root.join("Lieferung");
        assert!(collect(&s.manifest, "B", &p, &dest, &mut |_| {}).unwrap().is_empty());
        assert!(dest.join("V/Avid8.1/msmMMOB.mdb").exists());
        assert!(verify(&dest, &mut |_| {}).unwrap().is_empty());
        // zweiter Lauf setzt fort statt zu überschreiben
        assert!(collect(&s.manifest, "B", &p, &dest, &mut |_| {}).unwrap().is_empty());

        fs::write(dest.join("V/Avid8.1/a.mxf"), "XXX").unwrap();
        assert_eq!(verify(&dest, &mut |_| {}).unwrap().len(), 1);
        assert_eq!(collect(&s.manifest, "B", &p, &dest, &mut |_| {}).unwrap().len(), 1);

        // stille Veränderung (gleiche Größe, gleiches Datum) findet nur die vollständige Prüfung
        let a = folder.join("a.mxf");
        let mtime = fs::metadata(&a).unwrap().modified().unwrap();
        fs::write(&a, "zzz").unwrap();
        File::options().write(true).open(&a).unwrap().set_modified(mtime).unwrap();
        assert!(scan::scan(&[vol.clone()], false, Some(&cache), &mut |_| {}).unwrap().warnings.is_empty());
        assert_eq!(scan::scan(&[vol], true, Some(&cache), &mut |_| {}).unwrap().warnings.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

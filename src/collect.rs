//! Fehlende Dateien in einen Lieferordner sammeln, Lieferungen prüfen und einspielen.
use crate::plan::Plan;
use crate::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};

pub const DELIVERY_FORMAT: &str = "mediafiles-ferry-delivery";
pub const DELIVERY_FILE: &str = "ferry-lieferung.json";
pub const TASKS_FILE: &str = "Aufgaben.txt";
pub const BACKUP_DIR: &str = "Ferry-Backup";

#[derive(Serialize, Deserialize)]
pub struct Delivery {
    pub format: String,
    pub from: String,
    pub to: String,
    pub created: u64,
    pub files: Vec<Delivered>,
    /// Ordner, deren Datenbank mitgeliefert wird: "Volume/Ordner" -> MXF-Dateien, die danach genau dort liegen müssen
    #[serde(default)]
    pub db_folders: BTreeMap<String, Vec<String>>,
    pub tasks: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Delivered {
    /// relativ zum Lieferordner: Volume/Ordner/Datei
    pub path: String,
    pub size: u64,
    pub hash: String,
}

fn is_db(name: &str) -> bool {
    DB_FILES.iter().any(|d| d.eq_ignore_ascii_case(name))
}

/// Kopiert alle Dateien des Plans nach `dest/<Volume>/<Ordner>/`. Bricht bei einzelnen Fehlern nicht ab, sondern gibt sie zurück.
pub fn collect(local: &Manifest, to_system: &str, plan: &Plan, dest: &Path, progress: &mut dyn FnMut(&str)) -> io::Result<Vec<String>> {
    let mut files = vec![];
    let mut errors = vec![];
    let mut dbs: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
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
            Ok((size, hash)) => {
                files.push(Delivered { path: rel, size, hash });
                if c.hash.is_none() {
                    let source = vol.folders.iter().find(|f| f.name == c.from_folder);
                    let entry = dbs.entry(format!("{}/{}", c.to_volume, c.to_folder)).or_default();
                    entry.0 += 1;
                    entry.1 = source.map(|f| f.files.iter().map(|e| e.name.clone()).collect()).unwrap_or_default();
                }
            }
            Err(e) => errors.push(format!("{rel}: {e}")),
        }
    }
    // nur vollständige Datenbanken (beide Dateien) dürfen beim Einspielen ersetzt werden
    let db_folders: BTreeMap<String, Vec<String>> = dbs.into_iter().filter(|(_, (n, _))| *n == DB_FILES.len()).map(|(k, (_, f))| (k, f)).collect();

    fs::create_dir_all(dest)?;
    let tasks: Vec<String> = plan.tasks.iter().map(|t| t.to_string()).collect();
    let mut text = format!(
        "MediaFiles Ferry – Lieferung von {} für {to_system}\n\n\
         EINSPIELEN MIT FERRY: \"Lieferung einspielen\" wählen (Media Composer vorher schließen).\n\n\
         VON HAND EINSPIELEN:\n\
         1. Media Composer schließen.\n\
         2. Jeder Ordner dieser Lieferung heißt <Volume>/<Ordner>. Auf dem Volume den Ordner\n   \
            \"Avid MediaFiles/MXF/<Ordner>\" öffnen (fehlt er, neu anlegen).\n\
         3. NUR DEN INHALT des Lieferordners hineinkopieren – NIEMALS den ganzen Ordner auf einen\n   \
            bestehenden Ordner ziehen! Finder und Explorer ersetzen ihn sonst samt allen Mediendateien.\n\
         4. Fragt das System, ob .mxf-Dateien ersetzt werden sollen: NICHT ersetzen, abbrechen und prüfen.\n\
         5. msmFMID.pmr und msmMMOB.mdb nur in diesen Ordnern ersetzen:\n",
        local.system
    );
    for k in db_folders.keys() {
        text += &format!("   - {k}\n");
    }
    if db_folders.is_empty() {
        text += "   (keine)\n";
    }
    text += "\nAUFGABEN:\n";
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
        db_folders,
        tasks,
    };
    fs::write(dest.join(DELIVERY_FILE), serde_json::to_vec_pretty(&delivery)?)?;
    Ok(errors)
}

fn load(dest: &Path) -> io::Result<Delivery> {
    let d: Delivery = serde_json::from_slice(&fs::read(dest.join(DELIVERY_FILE))?)?;
    if d.format != DELIVERY_FORMAT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "keine Ferry-Lieferung"));
    }
    Ok(d)
}

/// Prüft eine Lieferung gegen ihr Manifest. Gibt die gefundenen Probleme zurück (leer = alles in Ordnung).
pub fn verify(dest: &Path, progress: &mut dyn FnMut(&str)) -> io::Result<Vec<String>> {
    let d = load(dest)?;
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

pub struct Imported {
    /// Probleme beim Einspielen (nicht eingespielte Dateien, nicht ersetzte Datenbanken)
    pub problems: Vec<String>,
    /// Aufgaben aus der Lieferung, die der User noch von Hand erledigen muss
    pub tasks: Vec<String>,
}

/// Spielt eine Lieferung in die MXF-Ordner ein. Fügt nur hinzu und überschreibt keine Mediendatei;
/// ersetzte Datenbanken wandern nach "Avid MediaFiles/Ferry-Backup/".
/// `map`: Volume-Name aus der Lieferung -> Einhängepunkt (ohne Eintrag: gleichnamiges Laufwerk).
pub fn import(dest: &Path, map: &HashMap<String, String>, progress: &mut dyn FnMut(&str)) -> io::Result<Imported> {
    let d = load(dest)?;
    let split = |p: &str| -> (String, String, String) {
        let mut it = p.splitn(3, '/').map(String::from);
        (it.next().unwrap_or_default(), it.next().unwrap_or_default(), it.next().unwrap_or_default())
    };

    // erst alle Volumes auflösen, damit nichts halb eingespielt wird
    let found = scan::find_volumes();
    let mut volumes: HashMap<String, PathBuf> = HashMap::new();
    for f in &d.files {
        let (vol, _, _) = split(&f.path);
        if volumes.contains_key(&vol) {
            continue;
        }
        let path = map
            .get(&vol)
            .map(PathBuf::from)
            .or_else(|| found.iter().find(|p| p.file_name().is_some_and(|n| key(&n.to_string_lossy()) == key(&vol))).cloned())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("Laufwerk \"{vol}\" nicht gefunden – anschließen oder zuordnen")))?;
        volumes.insert(vol, path);
    }

    let mut problems = vec![];
    let total = d.files.len();
    let mut done = 0;
    // Mediendateien zuerst: eine Datenbank darf erst ersetzt werden, wenn ihre Dateien da sind
    for f in d.files.iter().filter(|f| !is_db(&split(&f.path).2)) {
        done += 1;
        progress(&format!("[{done}/{total}] {}", f.path));
        let (vol, folder, name) = split(&f.path);
        let target = mxf_dir(&volumes[&vol]).join(&folder).join(&name);
        if let Err(e) = copy_verified(&dest.join(&f.path), &target, Some(&f.hash)) {
            problems.push(format!("{}: {e}", f.path));
        }
    }

    let backup = format!("{}_{}", yymmdd(now_secs()), now_secs());
    for (id, expected) in &d.db_folders {
        let (vol, folder, _) = split(&format!("{id}/"));
        let dir = mxf_dir(&volumes[&vol]).join(&folder);
        // Hat sich der Ordner seit dem Scan verändert, passt die gelieferte Datenbank nicht mehr.
        let want: HashSet<String> = expected.iter().map(|n| key(n)).collect();
        let have: HashSet<String> = fs::read_dir(&dir)?
            .flatten()
            .map(|e| key(&e.file_name().to_string_lossy()))
            .filter(|n| n.ends_with(".mxf"))
            .collect();
        if want != have {
            problems.push(format!("{id}: Ordner enthält andere Dateien als erwartet – Datenbank nicht ersetzt, bitte neu schreiben"));
            continue;
        }
        for f in d.files.iter().filter(|f| f.path.starts_with(&format!("{id}/")) && is_db(&split(&f.path).2)) {
            done += 1;
            progress(&format!("[{done}/{total}] {}", f.path));
            let name = split(&f.path).2;
            let target = dir.join(&name);
            if target.exists() && hash_file(&target)? != f.hash {
                let saved = volumes[&vol].join("Avid MediaFiles").join(BACKUP_DIR).join(&backup).join(&folder);
                fs::create_dir_all(&saved)?;
                fs::rename(&target, saved.join(&name))?;
            }
            if let Err(e) = copy_verified(&dest.join(&f.path), &target, Some(&f.hash)) {
                problems.push(format!("{}: {e} – alte Datenbank liegt in {BACKUP_DIR}/{backup}", f.path));
            }
        }
    }
    Ok(Imported { problems, tasks: d.tasks })
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
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Ziel existiert bereits mit anderem Inhalt – nicht überschrieben"));
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

    fn write_folder(dir: &Path, files: &[(&str, &str)]) {
        fs::create_dir_all(dir).unwrap();
        for (n, c) in files {
            fs::write(dir.join(n), c).unwrap();
        }
    }

    #[test]
    fn scan_collect_verify_end_to_end() {
        let root = tmp("e2e");
        let vol = root.join("V");
        let folder = mxf_dir(&vol).join("Avid8.1");
        fs::create_dir_all(folder.join("Creating/creating1")).unwrap();
        write_folder(&folder, &[("a.mxf", "aaa"), ("b.MXF", "bbb"), ("msmFMID.pmr", "db1"), ("msmMMOB.mdb", "db2"), ("notes.txt", "x")]);
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
        assert!(fs::read_to_string(dest.join(TASKS_FILE)).unwrap().contains("NUR DEN INHALT"));
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

    #[test]
    fn import_adds_files_and_replaces_db_only_when_folder_matches() {
        let root = tmp("import");
        let (a, b) = (root.join("A/V"), root.join("B/V"));
        write_folder(&mxf_dir(&a).join("1"), &[("x.mxf", "x"), ("y.mxf", "y"), ("msmFMID.pmr", "newF"), ("msmMMOB.mdb", "newM")]);
        write_folder(&mxf_dir(&a).join("2"), &[("p.mxf", "p"), ("msmFMID.pmr", "p1"), ("msmMMOB.mdb", "p2")]);
        write_folder(&mxf_dir(&b).join("1"), &[("x.mxf", "x"), ("msmFMID.pmr", "oldF"), ("msmMMOB.mdb", "oldM")]);
        write_folder(&mxf_dir(&b).join("2"), &[]);

        let sa = scan::scan(&[a], false, None, &mut |_| {}).unwrap().manifest;
        let sb = scan::scan(&[b.clone()], false, None, &mut |_| {}).unwrap().manifest;
        let p = plan::plan(&sa, &sb, &Options::default(), "261101");
        let dest = root.join("Lieferung");
        assert!(collect(&sa, "B", &p, &dest, &mut |_| {}).unwrap().is_empty());

        // seit dem Scan ist auf B in Ordner 2 eine Datei dazugekommen -> dessen DB darf nicht ersetzt werden
        fs::write(mxf_dir(&b).join("2/neu.mxf"), "n").unwrap();
        let map = [("V".to_string(), b.display().to_string())].into();
        let r = import(&dest, &map, &mut |_| {}).unwrap();
        assert_eq!(r.problems.len(), 1, "{:?}", r.problems);
        assert!(r.problems[0].starts_with("V/2:"));

        let read = |p: &str| fs::read_to_string(mxf_dir(&b).join(p)).unwrap();
        assert_eq!((read("1/y.mxf").as_str(), read("1/msmMMOB.mdb").as_str()), ("y", "newM"));
        assert_eq!(read("2/p.mxf"), "p");
        assert!(!mxf_dir(&b).join("2/msmMMOB.mdb").exists());
        let backup = fs::read_dir(b.join("Avid MediaFiles").join(BACKUP_DIR)).unwrap().next().unwrap().unwrap().path();
        assert_eq!(fs::read_to_string(backup.join("1/msmMMOB.mdb")).unwrap(), "oldM");

        // erneutes Einspielen ist harmlos
        assert_eq!(import(&dest, &map, &mut |_| {}).unwrap().problems.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

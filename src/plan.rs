//! Vergleich zweier Manifeste: was muss von hier zur Gegenseite, und was muss der User von Hand erledigen.
use crate::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

/// Avid legt ab etwa 5000 Dateien einen neuen Ordner an; größere Ordner sollten aufgeteilt werden.
pub const MAX_FILES: usize = 5000;

#[derive(Default)]
pub struct Options {
    /// lokaler Volume-Name -> Volume-Name auf der Gegenseite (ohne Eintrag: gleicher Name)
    pub volume_map: HashMap<String, String>,
    /// "Volume/Ordner" (lokal), die auf der Gegenseite einen eindeutigen Namen bekommen statt zusammengeführt zu werden
    pub rename: HashSet<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Copy {
    pub from_volume: String,
    pub from_folder: String,
    pub to_volume: String,
    pub to_folder: String,
    pub name: String,
    pub size: u64,
    /// None bei Datenbankdateien (die werden nicht gehasht)
    pub hash: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub enum Task {
    RebuildDb { system: String, volume: String, folder: String },
    Rebalance { system: String, volume: String, folder: String, files: usize },
    Conflict { system: String, volume: String, folder: String, name: String },
    Duplicates { system: String, places: Vec<String>, files: usize },
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Task::RebuildDb { system, volume, folder } => {
                write!(f, "[{system}] Datenbank neu schreiben: {volume}/{folder}")
            }
            Task::Rebalance { system, volume, folder, files } => {
                write!(f, "[{system}] Ordner aufteilen (rebalancen): {volume}/{folder} hat {files} Dateien")
            }
            Task::Conflict { system, volume, folder, name } => {
                write!(f, "[{system}] Gleicher Name, anderer Inhalt – nicht kopiert, bitte prüfen: {volume}/{folder}/{name}")
            }
            Task::Duplicates { system, places, files } => {
                write!(f, "[{system}] {files} doppelte Dateien in: {}", places.join(", "))
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct Plan {
    pub copies: Vec<Copy>,
    pub tasks: Vec<Task>,
    /// lokale "Volume/Ordner", deren Gegenstück eigene Dateien hat: User fragen, ob zusammenführen oder umbenennen
    pub collisions: Vec<String>,
    /// lokale Volumes ohne Gegenstück: User muss sie zuordnen
    pub unmapped: Vec<String>,
}

struct Proposal<'a> {
    id: String,
    from_volume: &'a str,
    from: &'a Folder,
    to_volume: String,
    to_folder: String,
    missing: Vec<&'a FileEntry>,
    existing: usize,
    db_ok: bool,
}

/// Was von `local` auf `remote` fehlt. Beide Seiten rechnen das unabhängig voneinander mit dem Manifest der anderen.
pub fn plan(local: &Manifest, remote: &Manifest, opts: &Options, date: &str) -> Plan {
    let mut p = Plan::default();

    // Eine Datei gilt als vorhanden, wenn sie irgendwo auf der Gegenseite mit gleichem Namen und Hash liegt.
    let mut remote_files: HashMap<String, HashSet<&str>> = HashMap::new();
    for e in remote.volumes.iter().flat_map(|v| &v.folders).flat_map(|f| &f.files) {
        remote_files.entry(key(&e.name)).or_default().insert(&e.hash);
    }
    let renames: HashSet<String> = opts.rename.iter().map(|r| key(r)).collect();
    let mut taken: HashMap<String, HashSet<String>> = HashMap::new();
    let mut proposals = vec![];

    for lv in &local.volumes {
        let mapped = opts.volume_map.get(&lv.name).cloned();
        let rv = remote.volumes.iter().find(|rv| key(&rv.name) == key(mapped.as_deref().unwrap_or(&lv.name)));
        let Some(to_volume) = mapped.or_else(|| rv.map(|rv| rv.name.clone())) else {
            p.unmapped.push(lv.name.clone());
            continue;
        };
        let taken = taken
            .entry(key(&to_volume))
            .or_insert_with(|| rv.iter().flat_map(|rv| &rv.folders).map(|f| key(&f.name)).collect());

        for f in &lv.folders {
            let mut missing = vec![];
            for e in &f.files {
                match remote_files.get(&key(&e.name)) {
                    None => missing.push(e),
                    Some(hashes) if hashes.contains(e.hash.as_str()) => {}
                    Some(_) => p.tasks.push(Task::Conflict {
                        system: local.system.clone(),
                        volume: lv.name.clone(),
                        folder: f.name.clone(),
                        name: e.name.clone(),
                    }),
                }
            }
            if missing.is_empty() {
                continue;
            }
            let id = format!("{}/{}", lv.name, f.name);
            let same = rv.and_then(|rv| rv.folders.iter().find(|rf| key(&rf.name) == key(&f.name)));
            let (to_folder, existing) = if renames.contains(&key(&id)) {
                (unique_name(&format!("{date}_{}_{}", safe_name(&local.system).replace('.', "_"), f.name.replace('.', "_")), taken), None)
            } else {
                (same.map_or_else(|| f.name.clone(), |s| s.name.clone()), same)
            };
            let mine: HashSet<(String, &str)> = f.files.iter().map(|e| (key(&e.name), e.hash.as_str())).collect();
            let subset = existing.is_none_or(|x| x.files.iter().all(|e| mine.contains(&(key(&e.name), e.hash.as_str()))));
            if !subset {
                p.collisions.push(id.clone());
            }
            let existing = existing.map_or(0, |x| x.files.len());
            // Die DB passt nur, wenn der Zielordner danach exakt dem Quellordner entspricht.
            let db_ok = f.has_db && subset && existing + missing.len() == f.files.len();
            proposals.push(Proposal { id, from_volume: &lv.name, from: f, to_volume: to_volume.clone(), to_folder, missing, existing, db_ok });
        }
    }

    // Mehrere lokale Ordner können im selben Zielordner landen (z. B. zwei lokale Volumes -> ein Volume drüben).
    let target = |pr: &Proposal| (key(&pr.to_volume), key(&pr.to_folder));
    let mut incoming: HashMap<(String, String), (usize, usize)> = HashMap::new(); // (Quellordner, neue Dateien)
    for pr in &proposals {
        let t = incoming.entry(target(pr)).or_default();
        t.0 += 1;
        t.1 += pr.missing.len();
    }
    for pr in &proposals {
        let (sources, added) = incoming[&target(pr)];
        if sources > 1 && !p.collisions.contains(&pr.id) {
            p.collisions.push(pr.id.clone());
        }
        let copy = |name: &str, size, hash| Copy {
            from_volume: pr.from_volume.into(),
            from_folder: pr.from.name.clone(),
            to_volume: pr.to_volume.clone(),
            to_folder: pr.to_folder.clone(),
            name: name.into(),
            size,
            hash,
        };
        p.copies.extend(pr.missing.iter().map(|e| copy(&e.name, e.size, Some(e.hash.clone()))));
        let (system, volume, folder) = (remote.system.clone(), pr.to_volume.clone(), pr.to_folder.clone());
        if pr.db_ok && sources == 1 {
            p.copies.extend(DB_FILES.iter().map(|db| copy(db, 0, None)));
        } else {
            push_once(&mut p.tasks, Task::RebuildDb { system: system.clone(), volume: volume.clone(), folder: folder.clone() });
        }
        if pr.existing + added > MAX_FILES {
            push_once(&mut p.tasks, Task::Rebalance { system, volume, folder, files: pr.existing + added });
        }
    }
    p.tasks.extend(hints(local));
    p
}

/// Hinweise zum eigenen Bestand: doppelte Dateien und übervolle Ordner.
pub fn hints(m: &Manifest) -> Vec<Task> {
    let mut tasks = vec![];
    let mut places: HashMap<(String, &str), Vec<String>> = HashMap::new();
    for v in &m.volumes {
        for f in &v.folders {
            if f.files.len() > MAX_FILES {
                tasks.push(Task::Rebalance { system: m.system.clone(), volume: v.name.clone(), folder: f.name.clone(), files: f.files.len() });
            }
            for e in &f.files {
                places.entry((key(&e.name), &e.hash)).or_default().push(format!("{}/{}", v.name, f.name));
            }
        }
    }
    // pro Kombination von Ordnern zusammenfassen, sonst werden es tausende Einzelhinweise
    let mut groups: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    for (_, pl) in places.into_iter().filter(|(_, pl)| pl.len() > 1) {
        *groups.entry(pl).or_default() += 1;
    }
    tasks.extend(groups.into_iter().map(|(places, files)| Task::Duplicates { system: m.system.clone(), places, files }));
    tasks
}

fn push_once(tasks: &mut Vec<Task>, t: Task) {
    if !tasks.contains(&t) {
        tasks.push(t);
    }
}

fn unique_name(base: &str, taken: &mut HashSet<String>) -> String {
    let mut name = base.to_string();
    for i in 2.. {
        if taken.insert(key(&name)) {
            break;
        }
        name = format!("{base}_{i}");
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dateien als "name" (Hash = Name) oder "name=hash"
    fn fo(name: &str, files: &[&str]) -> Folder {
        let files = files
            .iter()
            .map(|f| {
                let (n, h) = f.split_once('=').unwrap_or((f, f));
                FileEntry { name: n.into(), size: 1, hash: h.into() }
            })
            .collect();
        Folder { name: name.into(), has_db: true, files }
    }

    fn ma(system: &str, vols: Vec<(&str, Vec<Folder>)>) -> Manifest {
        let volumes = vols.into_iter().map(|(n, folders)| Volume { name: n.into(), path: String::new(), folders }).collect();
        Manifest { system: system.into(), volumes, ..Default::default() }
    }

    fn run(local: &Manifest, remote: &Manifest, opts: &Options) -> (Vec<String>, Plan) {
        let p = plan(local, remote, opts, "261031");
        (p.copies.iter().map(|c| format!("{}/{}/{}", c.to_volume, c.to_folder, c.name)).collect(), p)
    }

    fn rebuild(volume: &str, folder: &str) -> Task {
        Task::RebuildDb { system: "B".into(), volume: volume.into(), folder: folder.into() }
    }

    #[test]
    fn identical_folders_copy_nothing() {
        let a = ma("A", vec![("V", vec![fo("1", &["a", "b"])])]);
        let (copies, p) = run(&a, &a.clone(), &Options::default());
        assert!(copies.is_empty() && p.tasks.is_empty() && p.collisions.is_empty());
    }

    #[test]
    fn missing_folder_is_copied_with_db() {
        let a = ma("A", vec![("V", vec![fo("1", &["a", "b"])])]);
        let b = ma("B", vec![("V", vec![])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert_eq!(copies, ["V/1/a", "V/1/b", "V/1/msmFMID.pmr", "V/1/msmMMOB.mdb"]);
        assert!(p.tasks.is_empty());
    }

    #[test]
    fn superset_brings_its_db() {
        let a = ma("A", vec![("V", vec![fo("1", &["a", "b"])])]);
        let b = ma("B", vec![("V", vec![fo("1", &["a"])])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert_eq!(copies, ["V/1/b", "V/1/msmFMID.pmr", "V/1/msmMMOB.mdb"]);
        assert!(p.tasks.is_empty() && p.collisions.is_empty());
    }

    #[test]
    fn both_sides_unique_means_rebuild_task_and_collision() {
        let a = ma("A", vec![("V", vec![fo("1", &["a", "b"])])]);
        let b = ma("B", vec![("V", vec![fo("1", &["a", "c"])])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert_eq!(copies, ["V/1/b"]);
        assert_eq!(p.tasks, [rebuild("V", "1")]);
        assert_eq!(p.collisions, ["V/1"]);
    }

    #[test]
    fn file_elsewhere_counts_as_present() {
        let a = ma("A", vec![("V", vec![fo("1", &["a", "b", "c"])])]);
        let b = ma("B", vec![("V", vec![fo("1", &["a"]), fo("2", &["b"])]), ("W", vec![])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert_eq!(copies, ["V/1/c"]);
        assert_eq!(p.tasks, [rebuild("V", "1")]);
    }

    #[test]
    fn same_name_other_content_is_not_copied() {
        let a = ma("A", vec![("V", vec![fo("1", &["a=h1"])])]);
        let b = ma("B", vec![("V", vec![fo("1", &["a=h2"])])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert!(copies.is_empty());
        assert!(matches!(&p.tasks[..], [Task::Conflict { name, .. }] if name == "a"));
    }

    #[test]
    fn renamed_folder_gets_unique_name_without_db() {
        let a = ma("Mac Studio", vec![("V", vec![fo("1", &["a", "b"])])]);
        let b = ma("B", vec![("V", vec![fo("1", &["a", "c"]), fo("261031_Mac Studio_1", &[])])]);
        let opts = Options { rename: ["V/1".to_string()].into(), ..Default::default() };
        let (copies, p) = run(&a, &b, &opts);
        assert_eq!(copies, ["V/261031_Mac Studio_1_2/b"]);
        assert_eq!(p.tasks, [rebuild("V", "261031_Mac Studio_1_2")]);
        assert!(p.collisions.is_empty());
    }

    #[test]
    fn two_local_volumes_into_one_remote_volume() {
        let a = ma("A", vec![("V1", vec![fo("1", &["a"])]), ("V2", vec![fo("1", &["b"])])]);
        let b = ma("B", vec![("W", vec![])]);
        let opts = Options { volume_map: [("V1".into(), "W".into()), ("V2".into(), "W".into())].into(), ..Default::default() };
        let (copies, p) = run(&a, &b, &opts);
        assert_eq!(copies, ["W/1/a", "W/1/b"]);
        assert_eq!(p.tasks, [rebuild("W", "1")]);
        assert_eq!(p.collisions, ["V1/1", "V2/1"]);
    }

    #[test]
    fn unknown_volume_must_be_mapped() {
        let a = ma("A", vec![("X", vec![fo("1", &["a"])])]);
        let b = ma("B", vec![("Y", vec![])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert!(copies.is_empty());
        assert_eq!(p.unmapped, ["X"]);
    }

    #[test]
    fn names_compare_unicode_and_case_insensitive() {
        let a = ma("A", vec![("V", vec![fo("Avid8.1", &["A\u{0308}.mxf"])])]);
        let b = ma("B", vec![("V", vec![fo("avid8.1", &["\u{00e4}.mxf=A\u{0308}.mxf"])])]);
        assert!(run(&a, &b, &Options::default()).0.is_empty());
    }

    #[test]
    fn full_folders_and_duplicates_are_hinted() {
        let many: Vec<String> = (0..=MAX_FILES).map(|i| format!("f{i}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let a = ma("A", vec![("V", vec![fo("1", &many), fo("2", &["f1", "f2"])])]);
        let b = ma("B", vec![("V", vec![])]);
        let (copies, p) = run(&a, &b, &Options::default());
        assert_eq!(copies.len(), MAX_FILES + 1 + 2 + 2 + 2); // beide Ordner fehlen drüben: Dateien + je 2 DB
        let rebalance = |system: &str| Task::Rebalance { system: system.into(), volume: "V".into(), folder: "1".into(), files: MAX_FILES + 1 };
        assert!(p.tasks.contains(&rebalance("A")) && p.tasks.contains(&rebalance("B")));
        assert!(p.tasks.contains(&Task::Duplicates { system: "A".into(), places: vec!["V/1".into(), "V/2".into()], files: 2 }));
    }
}

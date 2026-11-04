//! Abläufe der Oberfläche: Vergleichen → Sammeln, Lieferung → Prüfen & Einspielen.
//! Der Zustand liegt in `Session`; jede Änderung baut die Modelle der Oberfläche neu auf.
use mediafiles_ferry::collect::{self, Delivery};
use mediafiles_ferry::plan::{self, Options, Plan};
use mediafiles_ferry::{DB_FILES, Manifest, key, local_time, now_secs, safe_name, scan, write_atomic, yymmdd};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Gemerkte Zuordnungen, damit man sie beim nächsten Mal nicht neu wählen muss.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Mappings {
    /// Gegenseite -> (eigenes Volume -> Volume drüben, "" = nicht senden)
    pub send: BTreeMap<String, BTreeMap<String, String>>,
    /// Absender -> (Volume der Lieferung -> eigenes Volume)
    pub receive: BTreeMap<String, BTreeMap<String, String>>,
}

impl Mappings {
    pub fn load(dir: &Path) -> Self {
        fs::read(dir.join("mappings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) {
        // Komfort: scheitert das Speichern, wird beim nächsten Mal neu gefragt.
        let _ = fs::create_dir_all(dir);
        let _ = write_atomic(&dir.join("mappings.json"), &serde_json::to_vec_pretty(self).unwrap_or_default());
    }
}

/// Eine Zuordnung: Name, Auswahlmöglichkeiten, gewählter Index (0 = noch offen).
pub struct Choice {
    pub name: String,
    pub options: Vec<String>,
    pub selected: usize,
}

#[derive(Default)]
pub struct Compare {
    pub local: Manifest,
    pub remote: Manifest,
    /// eigene Volumes ohne gleichnamiges Gegenstück; options = Volumes drüben, letzter Index = nicht senden
    pub drives: Vec<Choice>,
    /// Ordner auf beiden Seiten: (Volume/Ordner, -1 offen / 0 zusammenführen / 1 umbenennen)
    pub collisions: Vec<(String, i32)>,
    /// Anforderung aus MediaFiles Check: fester Plan, nichts zuzuordnen
    pub request: Option<Plan>,
}

impl Compare {
    pub fn new(local: Manifest, remote: Manifest, saved: &Mappings) -> Self {
        let remote_names: Vec<String> = remote.volumes.iter().map(|v| v.name.clone()).collect();
        let saved = saved.send.get(&remote.system);
        let drives = local
            .volumes
            .iter()
            .filter(|lv| !remote_names.iter().any(|r| key(r) == key(&lv.name)))
            .map(|lv| {
                let selected = match saved.and_then(|m| m.get(&lv.name)) {
                    Some(r) if r.is_empty() => remote_names.len() + 1,
                    Some(r) => remote_names.iter().position(|n| key(n) == key(r)).map_or(0, |i| i + 1),
                    None => 0,
                };
                Choice { name: lv.name.clone(), options: remote_names.clone(), selected }
            })
            .collect();
        let mut c = Compare { local, remote, drives, collisions: vec![], request: None };
        c.refresh_collisions();
        c
    }

    /// Anforderung: die Gegenseite ist der Anfordernde, der Plan steht fest.
    pub fn from_request(local: Manifest, req: &mediafiles_ferry::request::Request) -> Self {
        let plan = mediafiles_ferry::request::plan(&local, req);
        let remote = Manifest { system: req.system.clone(), created: req.created, ..Default::default() };
        Compare { local, remote, drives: vec![], collisions: vec![], request: Some(plan) }
    }

    fn options(&self, with_renames: bool) -> Options {
        let mut o = Options::default();
        for d in &self.drives {
            if (1..=d.options.len()).contains(&d.selected) {
                o.volume_map.insert(d.name.clone(), d.options[d.selected - 1].clone());
            }
        }
        if with_renames {
            o.rename = self.collisions.iter().filter(|c| c.1 == 1).map(|c| c.0.clone()).collect();
        }
        o
    }

    /// Kollisionen ohne Umbenennungen bestimmen, damit Zeilen nach der Wahl „Umbenennen“ nicht verschwinden.
    pub fn refresh_collisions(&mut self) {
        if self.request.is_some() {
            return;
        }
        let base = plan::plan(&self.local, &self.remote, &self.options(false), &yymmdd(now_secs()));
        let old = std::mem::take(&mut self.collisions);
        self.collisions = base.collisions.into_iter().map(|id| {
            let choice = old.iter().find(|c| c.0 == id).map_or(-1, |c| c.1);
            (id, choice)
        }).collect();
    }

    pub fn plan(&self) -> Plan {
        if let Some(p) = &self.request {
            return p.clone();
        }
        plan::plan(&self.local, &self.remote, &self.options(true), &yymmdd(now_secs()))
    }

    pub fn ready(&self) -> bool {
        self.drives.iter().all(|d| d.selected != 0) && self.collisions.iter().all(|c| c.1 >= 0)
    }

    pub fn remember(&self, m: &mut Mappings) {
        let entry = m.send.entry(self.remote.system.clone()).or_default();
        for d in &self.drives {
            match d.selected {
                0 => {}
                i if i <= d.options.len() => {
                    entry.insert(d.name.clone(), d.options[i - 1].clone());
                }
                _ => {
                    entry.insert(d.name.clone(), String::new());
                }
            }
        }
    }

    /// Lieferordner im gewählten Ordner, z. B. "261102-1830_Mac Studio_to_MacBook"
    pub fn delivery_dir(&self, parent: &Path) -> PathBuf {
        let name = format!("{}_{}_to_{}", local_time(now_secs(), "%y%m%d-%H%M"), safe_name(&self.local.system), safe_name(&self.remote.system));
        parent.join(name)
    }
}

/// Mediendateien (ohne Datenbanken) eines Plans: Anzahl und Bytes
pub fn media(plan: &Plan) -> (usize, u64) {
    plan.copies.iter().filter(|c| c.hash.is_some()).fold((0, 0), |(n, b), c| (n + 1, b + c.size))
}

pub struct Incoming {
    pub dir: PathBuf,
    pub delivery: Delivery,
    /// Volumes der Lieferung; options = eingehängte Laufwerke
    pub drives: Vec<Choice>,
}

impl Incoming {
    pub fn open(dir: PathBuf, saved: &Mappings) -> std::io::Result<Self> {
        let delivery = collect::load_delivery(&dir)?;
        let found: Vec<String> = scan::find_volumes().iter().map(|p| scan::volume_name(p)).collect();
        let saved = saved.receive.get(&delivery.from);
        let mut names: Vec<String> = vec![];
        for f in &delivery.files {
            let vol = f.path.split('/').next().unwrap_or_default().to_string();
            if !names.contains(&vol) {
                names.push(vol);
            }
        }
        let drives = names
            .into_iter()
            .map(|name| {
                let pick = |want: &str| found.iter().position(|f| key(f) == key(want)).map(|i| i + 1);
                let selected = pick(&name).or_else(|| saved.and_then(|m| m.get(&name)).and_then(|s| pick(s))).unwrap_or(0);
                Choice { name, options: found.clone(), selected }
            })
            .collect();
        Ok(Incoming { dir, delivery, drives })
    }

    pub fn ready(&self) -> bool {
        self.drives.iter().all(|d| d.selected != 0)
    }

    /// Volume der Lieferung -> Einhängepunkt
    pub fn map(&self) -> HashMap<String, String> {
        self.drives
            .iter()
            .filter(|d| d.selected != 0)
            .map(|d| (d.name.clone(), format!("/Volumes/{}", d.options[d.selected - 1])))
            .collect()
    }

    pub fn remember(&self, m: &mut Mappings) {
        let entry = m.receive.entry(self.delivery.from.clone()).or_default();
        for d in self.drives.iter().filter(|d| d.selected != 0) {
            entry.insert(d.name.clone(), d.options[d.selected - 1].clone());
        }
    }

    pub fn media(&self) -> (usize, u64) {
        let is_db = |p: &str| DB_FILES.iter().any(|d| p.ends_with(d));
        self.delivery.files.iter().filter(|f| !is_db(&f.path)).fold((0, 0), |(n, b), f| (n + 1, b + f.size))
    }
}

/// "[3/120] Volume/Ordner/Datei" -> 3/120
pub fn progress_value(line: &str) -> f32 {
    line.strip_prefix('[')
        .and_then(|r| r.split_once(']'))
        .and_then(|(n, _)| n.split_once('/'))
        .and_then(|(a, b)| Some(a.parse::<f32>().ok()? / b.parse::<f32>().ok()?.max(1.0)))
        .unwrap_or(0.0)
}

pub fn format_size(bytes: u64, lang: &str) -> String {
    let (v, unit) = if bytes >= 1_000_000_000 { (bytes as f64 / 1e9, "GB") } else { (bytes as f64 / 1e6, "MB") };
    let s = format!("{v:.1} {unit}");
    if lang == "de" { s.replace('.', ",") } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediafiles_ferry::{FileEntry, Folder, Volume};

    fn vol(name: &str, folders: &[(&str, &[&str])]) -> Volume {
        Volume {
            name: name.into(),
            path: String::new(),
            folders: folders
                .iter()
                .map(|(f, files)| Folder { name: (*f).into(), has_db: true, files: files.iter().map(|n| FileEntry { name: (*n).into(), size: 1, hash: (*n).into() }).collect() })
                .collect(),
        }
    }

    #[test]
    fn drives_without_counterpart_must_be_assigned_and_are_remembered() {
        let local = Manifest { system: "A".into(), volumes: vec![vol("Data", &[("1", &["a"])]), vol("Audio", &[("2", &["b"])])], ..Default::default() };
        let remote = Manifest { system: "B".into(), volumes: vec![vol("Data", &[]), vol("Audio_B", &[])], ..Default::default() };
        let mut c = Compare::new(local.clone(), remote.clone(), &Mappings::default());
        assert_eq!(c.drives.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Audio"]);
        assert!(!c.ready());
        assert_eq!(media(&c.plan()).0, 1, "unassigned drive is not sent");
        c.drives[0].selected = 2; // Audio_B
        assert!(c.ready());
        assert_eq!(media(&c.plan()).0, 2);
        let mut m = Mappings::default();
        c.remember(&mut m);
        assert_eq!(Compare::new(local, remote, &m).drives[0].selected, 2);
    }

    #[test]
    fn collision_rows_stay_after_choosing_rename() {
        let local = Manifest { system: "A".into(), volumes: vec![vol("V", &[("1", &["a", "b"])])], ..Default::default() };
        let remote = Manifest { system: "B".into(), volumes: vec![vol("V", &[("1", &["c"])])], ..Default::default() };
        let mut c = Compare::new(local, remote, &Mappings::default());
        assert_eq!(c.collisions, [("V/1".to_string(), -1)]);
        assert!(!c.ready());
        c.collisions[0].1 = 1;
        c.refresh_collisions();
        assert_eq!(c.collisions, [("V/1".to_string(), 1)]);
        assert!(c.ready());
        assert!(c.plan().copies.iter().all(|x| x.to_folder != "1"), "renamed folder gets a new name");
    }

    #[test]
    fn progress_and_sizes() {
        assert_eq!(progress_value("[3/4] V/1/a.mxf"), 0.75);
        assert_eq!(progress_value("V/1"), 0.0);
        assert_eq!(format_size(3_600_000_000, "de"), "3,6 GB");
        assert_eq!(format_size(29_000_000, "en"), "29.0 MB");
    }
}

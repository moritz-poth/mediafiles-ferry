//! Requests from MediaFiles Check: media files identified only by their Avid mob ID (the clips that use them are
//! offline on the requesting system). Ferry looks the IDs up in the Avid databases (msmFMID.pmr) of its drives and
//! collects the files like any other delivery.
use crate::plan::{Copy, Plan, Task};
use crate::*;
use std::collections::{BTreeSet, HashMap};

pub const REQUEST_FORMAT: &str = "mediafiles-ferry-request";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Request {
    pub format: String,
    pub version: u32,
    /// system that is missing the files
    pub system: String,
    pub created: u64,
    pub files: Vec<Wanted>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Wanted {
    /// Avid mob ID of the media file, 64 hex digits (32 bytes as in msmFMID.pmr)
    pub id: String,
    /// for people: a clip that uses the file, its bin and project
    #[serde(default)]
    pub clip: String,
    #[serde(default)]
    pub bin: String,
    #[serde(default)]
    pub project: String,
}

impl Request {
    pub fn load(path: &Path) -> io::Result<Request> {
        let not_request = || io::Error::new(io::ErrorKind::InvalidData, format!("{} is not a MediaFiles request", path.display()));
        let r: Request = serde_json::from_slice(&fs::read(path)?).map_err(|_| not_request())?;
        if r.format != REQUEST_FORMAT { Err(not_request()) } else { Ok(r) }
    }
}

/// Whether a .ferry file is a request (and not a manifest).
pub fn is_request(path: &Path) -> bool {
    #[derive(Deserialize)]
    struct Head {
        format: String,
    }
    fs::read(path).ok().and_then(|b| serde_json::from_slice::<Head>(&b).ok()).is_some_and(|h| h.format == REQUEST_FORMAT)
}

/// Media file names and their mob IDs from a msmFMID.pmr (format: see MediaFiles Check, src/lib.rs).
/// Block 2 holds every name in UTF-8; block 1 (Mac Roman, incomplete) is skipped.
pub fn read_pmr(data: &[u8]) -> Result<Vec<([u8; 32], String)>, String> {
    let mut p = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        let end = p.checked_add(n).filter(|&e| e <= data.len()).ok_or("file ends too early")?;
        let s = &data[p..end];
        p = end;
        Ok(s)
    };
    let u32le = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap()) as usize;
    let u16le = |b: &[u8]| u16::from_le_bytes(b.try_into().unwrap()) as usize;
    take(8)?;
    let count = u32le(take(4)?);
    for _ in 0..count {
        take(32)?;
        let n = u16le(take(2)?);
        take(n)?;
        let n = u16le(take(2)?);
        take(n + 36)?;
    }
    take(4)?;
    let count2 = u32le(take(4)?);
    let mut out = Vec::with_capacity(count2.min(100_000));
    for _ in 0..count2 {
        let id: [u8; 32] = take(32)?.try_into().unwrap();
        let n = u32le(take(4)?).checked_sub(2).ok_or("unexpected name length")?;
        let name = String::from_utf8(take(n)?.to_vec()).map_err(|_| "a name is not valid UTF-8")?;
        let n = u16le(take(2)?);
        take(n + 36)?;
        out.push((id, name));
    }
    Ok(out)
}

pub fn hex(id: &[u8; 32]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// Which of the requested files this system has (by the last scan) and where; the rest becomes a task.
pub fn plan(local: &Manifest, req: &Request) -> Plan {
    // mob ID -> (volume, folder, file) from the databases of every scanned folder
    let mut index: HashMap<String, (usize, usize, String)> = HashMap::new();
    for (vi, v) in local.volumes.iter().enumerate() {
        for (fi, f) in v.folders.iter().enumerate() {
            let Ok(data) = fs::read(mxf_dir(Path::new(&v.path)).join(&f.name).join(DB_FILES[0])) else { continue };
            for (id, name) in read_pmr(&data).unwrap_or_default() {
                index.entry(hex(&id)).or_insert((vi, fi, name));
            }
        }
    }
    let mut p = Plan::default();
    let mut folders = BTreeSet::new();
    let mut missing = vec![];
    let mut taken = BTreeSet::new();
    for w in &req.files {
        let found = index.get(&w.id.to_lowercase()).and_then(|(vi, fi, name)| {
            let (v, f) = (&local.volumes[*vi], &local.volumes[*vi].folders[*fi]);
            f.files.iter().find(|e| key(&e.name) == key(name)).map(|e| (v, f, e))
        });
        match found {
            Some((v, f, e)) => {
                if taken.insert((v.name.clone(), f.name.clone(), e.name.clone())) {
                    folders.insert((v.name.clone(), f.name.clone()));
                    p.copies.push(Copy {
                        from_volume: v.name.clone(),
                        from_folder: f.name.clone(),
                        to_volume: v.name.clone(),
                        to_folder: f.name.clone(),
                        name: e.name.clone(),
                        size: e.size,
                        hash: Some(e.hash.clone()),
                    });
                }
            }
            None => missing.push(w),
        }
    }
    // media files only, never a database: the receiving folders need a new database
    for (volume, folder) in folders {
        p.tasks.push(Task::RebuildDb { system: req.system.clone(), volume, folder });
    }
    if !missing.is_empty() {
        let mut clips: Vec<String> = missing.iter().map(|w| w.clip.clone()).filter(|c| !c.is_empty()).collect();
        clips.sort();
        clips.dedup();
        p.tasks.push(Task::NotFound { system: local.system.clone(), files: missing.len(), clips });
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pmr(entries: &[([u8; 32], &str)]) -> Vec<u8> {
        let mut d = vec![];
        d.extend(1961u32.to_le_bytes());
        d.extend(8u32.to_le_bytes());
        d.extend(0u32.to_le_bytes()); // block 1 left empty (it is skipped anyway)
        d.extend(16u32.to_le_bytes());
        d.extend((entries.len() as u32).to_le_bytes());
        for (id, name) in entries {
            d.extend(id);
            d.extend((name.len() as u32 + 2).to_le_bytes());
            d.extend(name.as_bytes());
            d.extend(2u16.to_le_bytes());
            d.extend(b"WG");
            d.extend([0u8; 36]);
        }
        d
    }

    #[test]
    fn requested_ids_are_found_through_the_databases() {
        let root = std::env::temp_dir().join(format!("ferry-request-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder = mxf_dir(&root).join("1");
        fs::create_dir_all(&folder).unwrap();
        let (a, b, gone) = ([1u8; 32], [2u8; 32], [3u8; 32]);
        fs::write(folder.join(DB_FILES[0]), pmr(&[(a, "Kölsch A01.mxf"), (b, "b.mxf")])).unwrap();
        let local = Manifest {
            system: "Sender".into(),
            volumes: vec![Volume {
                name: "V".into(),
                path: root.display().to_string(),
                folders: vec![Folder { name: "1".into(), has_db: true, files: vec![FileEntry { name: "Kölsch A01.mxf".into(), size: 5, hash: "h".into() }] }],
            }],
            ..Default::default()
        };
        let want = |id: &[u8; 32], clip: &str| Wanted { id: hex(id), clip: clip.into(), ..Default::default() };
        let req = Request { format: REQUEST_FORMAT.into(), system: "Me".into(), files: vec![want(&a, "Clip A"), want(&a, "Clip A2"), want(&b, "Clip B"), want(&gone, "Clip C")], ..Default::default() };
        let p = plan(&local, &req);
        assert_eq!(p.copies.len(), 1, "same file requested twice is copied once");
        assert_eq!((p.copies[0].name.as_str(), p.copies[0].hash.as_deref()), ("Kölsch A01.mxf", Some("h")));
        // b is in the database but not on disk (not in the scan), gone is nowhere
        assert_eq!(p.tasks, [
            Task::RebuildDb { system: "Me".into(), volume: "V".into(), folder: "1".into() },
            Task::NotFound { system: "Sender".into(), files: 2, clips: vec!["Clip B".into(), "Clip C".into()] },
        ]);
        let path = root.join("r.ferry");
        fs::write(&path, serde_json::to_vec(&req).unwrap()).unwrap();
        assert!(is_request(&path) && Request::load(&path).is_ok());
        fs::remove_dir_all(&root).unwrap();
    }
}

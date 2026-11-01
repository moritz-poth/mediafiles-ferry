//! Kommandozeile zum Testen des Kerns; die Oberfläche folgt in Phase 2.
use mediafiles_ferry::plan::{self, Options, Plan};
use mediafiles_ferry::{Manifest, collect, now_secs, scan, yymmdd};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
MediaFiles Ferry – Testversion (Kommandozeile)

  ferry volumes                                  gefundene Avid-Laufwerke anzeigen
  ferry scan [--full] [-o DATEI] [LAUFWERK...]   scannen und Manifest schreiben (ohne Angabe: alle Laufwerke)
  ferry plan LOKAL.json GEGENSEITE.json [Optionen]              zeigen, was fehlt
  ferry collect LOKAL.json GEGENSEITE.json ZIELORDNER [Optionen] fehlende Dateien sammeln
  ferry verify LIEFERORDNER                      Lieferung prüfen
  ferry import LIEFERORDNER [--map VOLUME=PFAD]  Lieferung in die MXF-Ordner einspielen

  --full                   alles neu hashen und auf Beschädigung prüfen
  --map LOKAL=GEGENSEITE   Volume zuordnen (bei import: Volume der Lieferung=Einhängepunkt)
  --rename VOLUME/ORDNER   Ordner drüben eindeutig benennen statt zusammenführen
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let (mut pos, mut opts, mut full, mut out) = (vec![], Options::default(), false, None);
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--full" => full = true,
            "-o" => out = Some(it.next().ok_or("-o braucht einen Dateinamen")?),
            "--map" => {
                let m = it.next().ok_or("--map braucht LOKAL=GEGENSEITE")?;
                let (l, r) = m.split_once('=').ok_or("--map braucht LOKAL=GEGENSEITE")?;
                opts.volume_map.insert(l.into(), r.into());
            }
            "--rename" => {
                opts.rename.insert(it.next().ok_or("--rename braucht VOLUME/ORDNER")?);
            }
            _ => pos.push(a),
        }
    }
    let progress = &mut |s: &str| eprintln!("  {s}");

    match pos.first().map(String::as_str) {
        Some("volumes") => scan::find_volumes().iter().for_each(|v| println!("{}", v.display())),
        Some("scan") => {
            let vols: Vec<PathBuf> = if pos.len() > 1 { pos[1..].iter().map(PathBuf::from).collect() } else { scan::find_volumes() };
            if vols.is_empty() {
                return Err("keine Laufwerke mit \"Avid MediaFiles/MXF\" gefunden".into());
            }
            let r = scan::scan(&vols, full, scan::app_dir().map(|d| d.join("cache")).as_deref(), progress)?;
            for w in &r.warnings {
                eprintln!("Warnung: {w}");
            }
            let m = &r.manifest;
            let out = out.unwrap_or_else(|| format!("{}.ferry.json", m.system));
            m.save(Path::new(&out))?;
            let files: usize = m.volumes.iter().flat_map(|v| &v.folders).map(|f| f.files.len()).sum();
            println!("{} Volumes, {files} Dateien -> {out}", m.volumes.len());
        }
        Some(cmd @ ("plan" | "collect")) => {
            let (Some(l), Some(r)) = (pos.get(1), pos.get(2)) else { return Err(USAGE.into()) };
            let (local, remote) = (Manifest::load(l.as_ref())?, Manifest::load(r.as_ref())?);
            let p = plan::plan(&local, &remote, &opts, &yymmdd(now_secs()));
            print_plan(&p, &remote.system);
            if !p.unmapped.is_empty() {
                let names: Vec<_> = remote.volumes.iter().map(|v| v.name.as_str()).collect();
                return Err(format!("Volumes ohne Gegenstück: {} – mit --map zuordnen (drüben: {})", p.unmapped.join(", "), names.join(", ")).into());
            }
            if cmd == "collect" {
                let dest = pos.get(3).ok_or(USAGE)?;
                let errors = collect::collect(&local, &remote.system, &p, dest.as_ref(), progress)?;
                for e in &errors {
                    eprintln!("Nicht kopiert: {e}");
                }
                println!("Lieferung in {dest} – Aufgaben siehe {}", collect::TASKS_FILE);
                if !errors.is_empty() {
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        Some("verify") => {
            let problems = collect::verify(pos.get(1).ok_or(USAGE)?.as_ref(), progress)?;
            problems.iter().for_each(|p| println!("Problem: {p}"));
            println!("{}", if problems.is_empty() { "Lieferung vollständig und unbeschädigt." } else { "Lieferung fehlerhaft!" });
            if !problems.is_empty() {
                return Ok(ExitCode::FAILURE);
            }
        }
        Some("import") => {
            eprintln!("Media Composer muss geschlossen sein.");
            let r = collect::import(pos.get(1).ok_or(USAGE)?.as_ref(), &opts.volume_map, progress)?;
            r.problems.iter().for_each(|p| println!("Problem: {p}"));
            r.tasks.iter().for_each(|t| println!("Aufgabe: {t}"));
            println!("{}", if r.problems.is_empty() { "Lieferung eingespielt." } else { "Lieferung mit Problemen eingespielt – siehe oben." });
            if !r.problems.is_empty() {
                return Ok(ExitCode::FAILURE);
            }
        }
        _ => {
            eprint!("{USAGE}");
            return Ok(ExitCode::FAILURE);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn print_plan(p: &Plan, to: &str) {
    let bytes: u64 = p.copies.iter().map(|c| c.size).sum();
    println!("Für {to} zu sammeln: {} Dateien, {:.1} GB", p.copies.len(), bytes as f64 / 1e9);
    for c in &p.collisions {
        println!("Ordner gibt es auf beiden Seiten mit eigenen Dateien: {c} (zusammenführen oder --rename)");
    }
    for t in &p.tasks {
        println!("Aufgabe: {t}");
    }
}

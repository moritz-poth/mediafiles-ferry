//! Command line for the core (scripting and testing); the app is src/app.rs.
use mediafiles_ferry::plan::{self, Options, Plan};
use mediafiles_ferry::{Manifest, collect, manifest_file_name, now_secs, scan, yymmdd};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
MediaFiles Ferry – command line

  ferry volumes                                  list drives with Avid media
  ferry scan [--full] [-o FILE] [DRIVE...]       scan and write a manifest (default: all drives)
  ferry plan MINE.ferry OTHER.ferry [options]              show what the other side is missing
  ferry collect MINE.ferry OTHER.ferry TARGET [options]    collect the missing files into a delivery
  ferry verify DELIVERY                          verify a delivery
  ferry import DELIVERY [--map VOLUME=PATH]      import a delivery into the MXF folders

  --full                   rehash everything and check for damage
  --map LOCAL=OTHER        assign a volume (import: delivery volume=mount point)
  --rename VOLUME/FOLDER   give the folder a unique name on the other side instead of merging
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e}");
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
            "-o" => out = Some(it.next().ok_or("-o needs a file name")?),
            "--map" => {
                let m = it.next().ok_or("--map needs LOCAL=OTHER")?;
                let (l, r) = m.split_once('=').ok_or("--map needs LOCAL=OTHER")?;
                opts.volume_map.insert(l.into(), r.into());
            }
            "--rename" => {
                opts.rename.insert(it.next().ok_or("--rename needs VOLUME/FOLDER")?);
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
                return Err("no drives with \"Avid MediaFiles/MXF\" found".into());
            }
            let r = scan::scan(&vols, full, scan::app_dir().map(|d| d.join("cache")).as_deref(), progress)?;
            for w in &r.warnings {
                eprintln!("Warning: {w}");
            }
            let m = &r.manifest;
            let out = out.unwrap_or_else(|| manifest_file_name(m));
            m.save(Path::new(&out))?;
            let files: usize = m.volumes.iter().flat_map(|v| &v.folders).map(|f| f.files.len()).sum();
            println!("{} volumes, {files} files -> {out}", m.volumes.len());
        }
        Some(cmd @ ("plan" | "collect")) => {
            let (Some(l), Some(r)) = (pos.get(1), pos.get(2)) else { return Err(USAGE.into()) };
            let (local, remote) = (Manifest::load(l.as_ref())?, Manifest::load(r.as_ref())?);
            let p = plan::plan(&local, &remote, &opts, &yymmdd(now_secs()));
            print_plan(&p, &remote.system);
            if !p.unmapped.is_empty() {
                let names: Vec<_> = remote.volumes.iter().map(|v| v.name.as_str()).collect();
                return Err(format!("volumes without counterpart: {} – assign with --map (other side: {})", p.unmapped.join(", "), names.join(", ")).into());
            }
            if cmd == "collect" {
                let dest = pos.get(3).ok_or(USAGE)?;
                let errors = collect::collect(&local, &remote.system, &p, dest.as_ref(), "en", progress)?;
                for e in &errors {
                    eprintln!("Not copied: {e}");
                }
                println!("Delivery in {dest} – tasks: {}", collect::tasks_file("en"));
                if !errors.is_empty() {
                    return Ok(ExitCode::FAILURE);
                }
            }
        }
        Some("verify") => {
            let problems = collect::verify(pos.get(1).ok_or(USAGE)?.as_ref(), progress)?;
            problems.iter().for_each(|p| println!("Problem: {p}"));
            println!("{}", if problems.is_empty() { "Delivery complete and undamaged." } else { "Delivery is faulty!" });
            if !problems.is_empty() {
                return Ok(ExitCode::FAILURE);
            }
        }
        Some("import") => {
            eprintln!("Media Composer must be closed.");
            let r = collect::import(pos.get(1).ok_or(USAGE)?.as_ref(), &opts.volume_map, progress)?;
            r.problems.iter().for_each(|p| println!("Problem: {p}"));
            r.tasks.iter().for_each(|t| println!("Task: {t}"));
            println!("{}", if r.problems.is_empty() { "Delivery imported." } else { "Delivery imported with problems – see above." });
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
    println!("To collect for {to}: {} files, {:.1} GB", p.copies.len(), bytes as f64 / 1e9);
    for c in &p.collisions {
        println!("Folder exists on both sides with files of its own: {c} (merge or --rename)");
    }
    for t in &p.tasks {
        println!("Task: {t}");
    }
}

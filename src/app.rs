//! Grafische Oberfläche. Phase 2, Schritt 1: Startbildschirm.
#![cfg_attr(windows, windows_subsystem = "windows")]

use mediafiles_ferry::{Manifest, key, scan};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

/// Neue Sprache: hier eintragen und translations/<code>/LC_MESSAGES/mediafiles-ferry.po anlegen.
const LANGUAGES: [(&str, &str, char); 2] = [("en", "English", ','), ("de", "Deutsch", '.')];

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    language: String,
    dark: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { language: "en".into(), dark: true }
    }
}

fn dir() -> PathBuf {
    scan::app_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn manifest_path() -> PathBuf {
    dir().join("last-manifest.json")
}

fn save_settings(s: &Settings) {
    // Einstellungen sind Komfort; scheitert das Speichern, gilt beim nächsten Start der Standard.
    let _ = fs::create_dir_all(dir());
    let _ = fs::write(dir().join("settings.json"), serde_json::to_vec_pretty(s).unwrap_or_default());
}

fn minutes_since_scan() -> i32 {
    fs::metadata(manifest_path()).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(-1, |d| (d.as_secs() / 60) as i32)
}

fn group_digits(n: usize, lang: &str) -> String {
    let sep = LANGUAGES.iter().find(|l| l.0 == lang).map_or(',', |l| l.2);
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Laufwerksliste: angeschlossene Laufwerke plus die aus dem letzten Scan, die gerade fehlen.
fn refresh(app: &AppWindow, lang: &str) {
    let manifest = Manifest::load(&manifest_path()).ok();
    let scanned = |name: &str| {
        manifest.as_ref().and_then(|m| m.volumes.iter().find(|v| key(&v.name) == key(name))).map(|v| v.folders.iter().map(|f| f.files.len()).sum::<usize>())
    };
    let files = |n: Option<usize>| n.map(|n| group_digits(n, lang)).unwrap_or_default().into();
    let mut rows: Vec<VolumeRow> = scan::find_volumes()
        .iter()
        .map(|p| {
            let name = scan::volume_name(p);
            VolumeRow { files: files(scanned(&name)), name: name.into(), online: true }
        })
        .collect();
    for v in manifest.iter().flat_map(|m| &m.volumes) {
        if !rows.iter().any(|r| key(&r.name) == key(&v.name)) {
            rows.push(VolumeRow { name: v.name.clone().into(), files: files(scanned(&v.name)), online: false });
        }
    }
    app.set_volumes(ModelRc::new(VecModel::from(rows)));
    app.set_has_manifest(manifest.is_some());
    app.set_minutes_since_scan(minutes_since_scan());
}

fn main() -> Result<(), slint::PlatformError> {
    let settings: Settings = fs::read(dir().join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let app = AppWindow::new()?;
    let _ = slint::select_bundled_translation(&settings.language);
    app.set_language(settings.language.clone().into());
    app.set_dark(settings.dark);
    app.set_system_name(scan::computer_name().into());
    let languages: Vec<Language> = LANGUAGES.iter().map(|l| Language { code: l.0.into(), name: l.1.into() }).collect();
    app.set_languages(ModelRc::new(VecModel::from(languages)));
    refresh(&app, &settings.language);
    let settings = Rc::new(RefCell::new(settings));

    app.on_select_language({
        let (weak, settings) = (app.as_weak(), settings.clone());
        move |code| {
            let app = weak.unwrap();
            let _ = slint::select_bundled_translation(&code);
            app.set_language(code.clone());
            settings.borrow_mut().language = code.to_string();
            save_settings(&settings.borrow());
            refresh(&app, &code);
        }
    });

    app.on_dark_changed({
        let settings = settings.clone();
        move |dark| {
            settings.borrow_mut().dark = dark;
            save_settings(&settings.borrow());
        }
    });

    app.on_scan({
        let weak = app.as_weak();
        move || {
            let app = weak.unwrap();
            app.set_scanning(true);
            app.set_warnings(0);
            app.set_error("".into());
            let (vols, weak) = (scan::find_volumes(), weak.clone());
            std::thread::spawn(move || {
                let progress_to = weak.clone();
                let result = scan::scan(&vols, false, Some(&dir().join("cache")), &mut |p| {
                    let (p, w) = (p.to_string(), progress_to.clone());
                    let _ = slint::invoke_from_event_loop(move || w.upgrade().into_iter().for_each(|a| a.set_progress(p.clone().into())));
                })
                .and_then(|r| r.manifest.save(&manifest_path()).map(|_| r.warnings.len()));
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak.upgrade() else { return };
                    app.set_scanning(false);
                    match result {
                        Ok(warnings) => app.set_warnings(warnings as i32),
                        Err(e) => app.set_error(e.to_string().into()),
                    }
                    refresh(&app, &app.get_language());
                });
            });
        }
    });

    app.on_save_manifest({
        let weak = app.as_weak();
        move || {
            let name = format!("{}.ferry.json", scan::computer_name());
            if let Some(target) = rfd::FileDialog::new().set_file_name(name).add_filter("Ferry manifest", &["json"]).save_file() {
                if let Err(e) = fs::copy(manifest_path(), &target) {
                    weak.unwrap().set_error(format!("{}: {e}", target.display()).into());
                }
            }
        }
    });

    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(30), {
        let weak = app.as_weak();
        move || weak.upgrade().into_iter().for_each(|a| a.set_minutes_since_scan(minutes_since_scan()))
    });

    app.run()
}

#[cfg(test)]
mod tests {
    #[test]
    fn digits_are_grouped_per_language() {
        assert_eq!(super::group_digits(118842, "de"), "118.842");
        assert_eq!(super::group_digits(1234567, "en"), "1,234,567");
        assert_eq!(super::group_digits(999, "de"), "999");
    }
}

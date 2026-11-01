//! Grafische Oberfläche. Phase 2, Schritt 1: Startbildschirm; Sprache und Darstellung im Menü Settings.
#![cfg_attr(windows, windows_subsystem = "windows")]

use mediafiles_ferry::{MANIFEST_EXT, Manifest, key, manifest_file_name, scan, write_atomic};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

/// (Code, Name in der eigenen Sprache, Tausendertrennzeichen)
/// Neue Sprache: hier eintragen und translations/<code>/LC_MESSAGES/mediafiles-ferry.po anlegen.
const LANGUAGES: [(&str, &str, char); 2] = [("en", "English", ','), ("de", "Deutsch", '.')];

// Zeichenzahlen passend zur festen Fensterbreite (Metrics.window-width in ui/app.slint)
const NAME_CHARS: usize = 20;
const LINE_CHARS: usize = 44;
const TITLE_CHARS: usize = 38;

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
    let _ = write_atomic(&dir().join("settings.json"), &serde_json::to_vec_pretty(s).unwrap_or_default());
}

fn minutes_since_scan() -> i32 {
    fs::metadata(manifest_path()).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(-1, |d| (d.as_secs() / 60) as i32)
}

/// Kürzt lange Namen in der Mitte, damit Anfang und Ende sichtbar bleiben. Die Schrift ist gleich breit, Zeichen = Breite.
fn shorten(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let head = (max - 1) / 2;
    let tail = max - 1 - head;
    chars[..head].iter().chain(['…'].iter()).chain(chars[chars.len() - tail..].iter()).collect()
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
    let online: Vec<String> = scan::find_volumes().iter().map(|p| scan::volume_name(p)).collect();
    let offline = manifest.iter().flat_map(|m| &m.volumes).map(|v| &v.name).filter(|n| !online.iter().any(|o| key(o) == key(n)));
    let rows: Vec<VolumeRow> = online
        .iter()
        .map(|n| (n, true))
        .chain(offline.map(|n| (n, false)))
        .map(|(n, online)| VolumeRow { name: shorten(n, NAME_CHARS).into(), files: files(scanned(n)), online })
        .collect();
    app.set_volumes(ModelRc::new(VecModel::from(rows)));
    app.set_has_manifest(manifest.is_some());
    app.set_minutes_since_scan(minutes_since_scan());
}

fn current_language(app: &AppWindow) -> String {
    app.get_language().to_string()
}

/// Setzt die Menü-Modelle neu: neue Einträge bekommen frische Häkchen (ein Klick hakt sonst nur den geklickten Eintrag um).
fn set_menus(app: &AppWindow, s: &Settings) {
    let languages: Vec<MenuChoice> = LANGUAGES.iter().map(|l| MenuChoice { title: l.1.into(), checked: l.0 == s.language }).collect();
    app.set_languages(ModelRc::new(VecModel::from(languages)));
    app.set_appearance(ModelRc::new(VecModel::from(vec![s.dark, !s.dark])));
}

fn main() -> Result<(), slint::PlatformError> {
    let settings: Settings = fs::read(dir().join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let app = AppWindow::new()?;
    let _ = slint::select_bundled_translation(&settings.language);
    app.set_language(settings.language.clone().into());
    app.set_dark(settings.dark);
    app.set_system_name(shorten(&scan::computer_name(), TITLE_CHARS).into());
    set_menus(&app, &settings);
    refresh(&app, &settings.language);
    let settings = Rc::new(RefCell::new(settings));

    // Änderungen aus dem Menü erst nach dem Klick ausführen: Sie benennen die Menüeinträge um,
    // und das während der Menü-Verarbeitung von macOS führte zum Absturz.
    app.on_select_language({
        let (weak, settings) = (app.as_weak(), settings.clone());
        move |index| {
            let (weak, settings) = (weak.clone(), settings.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let Some(app) = weak.upgrade() else { return };
                let code = LANGUAGES.get(index as usize).map_or("en", |l| l.0);
                let _ = slint::select_bundled_translation(code);
                app.set_language(code.into());
                settings.borrow_mut().language = code.to_string();
                save_settings(&settings.borrow());
                set_menus(&app, &settings.borrow());
                refresh(&app, code);
            });
        }
    });

    app.on_select_dark({
        let (weak, settings) = (app.as_weak(), settings.clone());
        move |dark| {
            let (weak, settings) = (weak.clone(), settings.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let Some(app) = weak.upgrade() else { return };
                app.set_dark(dark);
                settings.borrow_mut().dark = dark;
                save_settings(&settings.borrow());
                set_menus(&app, &settings.borrow());
            });
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
                    let _ = slint::invoke_from_event_loop(move || w.upgrade().into_iter().for_each(|a| a.set_progress(shorten(&p, LINE_CHARS - 12).into())));
                })
                .and_then(|r| r.manifest.save(&manifest_path()).map(|_| r.warnings.len()));
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = weak.upgrade() else { return };
                    app.set_scanning(false);
                    match result {
                        Ok(warnings) => app.set_warnings(warnings as i32),
                        Err(e) => app.set_error(shorten(&e.to_string(), LINE_CHARS).into()),
                    }
                    refresh(&app, &current_language(&app));
                });
            });
        }
    });

    app.on_save_manifest({
        let weak = app.as_weak();
        move || {
            let Ok(m) = Manifest::load(&manifest_path()) else { return };
            let dialog = rfd::FileDialog::new().set_file_name(manifest_file_name(&m)).add_filter("MediaFiles Ferry Manifest", &[MANIFEST_EXT]);
            if let Some(target) = dialog.save_file() {
                if let Err(e) = fs::copy(manifest_path(), &target) {
                    weak.unwrap().set_error(shorten(&format!("{}: {e}", target.display()), LINE_CHARS).into());
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

    #[test]
    fn long_names_are_shortened_in_the_middle() {
        assert_eq!(super::shorten("Avid8.1", 10), "Avid8.1");
        assert_eq!(super::shorten("Mac Studio von Mathias.100", 11), "Mac S…s.100");
        assert_eq!(super::shorten("Alex’s Mac Studio.1", 19).chars().count(), 19);
    }
}

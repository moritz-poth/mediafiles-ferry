# MediaFiles Ferry – Hinweise für die Arbeit am Projekt

Rust (Kern + CLI `ferry`) und Slint (Oberfläche `ferry-app`). macOS zuerst, Windows folgt. GPL-3.0.
Benutzer sind Editoren und Schnittassistenten ohne Technikbezug: einfach, minimalistisch, maximale Datensicherheit.

## Feste Regeln
- Ferry löscht nie Mediendateien und überschreibt nie eine vorhandene Mediendatei. Löschen bleibt Handarbeit (höchstens Verschieben in `Trash/<Ordner>/`).
- Datenbank (`msmFMID.pmr`, `msmMMOB.mdb`) wird nie gelöscht und nur mitgeliefert/ersetzt, wenn der Zielordner danach exakt dem Quellordner entspricht; sonst Aufgabe „Datenbank neu schreiben“.
- Eine Datei gilt als vorhanden, wenn sie irgendwo drüben mit gleichem Namen und Hash liegt. Gleicher Name + anderer Hash: nicht kopieren, Aufgabe.
- Alles, was der User von Hand tun muss, landet in der Aufgabenliste (`Aufgaben.txt`). Die Anleitung zum manuellen Einspielen („nur den Inhalt kopieren, nie den Ordner ersetzen“) steht immer darin.
- Vergleich von Namen über `key()` (NFC + Kleinschreibung). Verwaltungsdateien immer über `write_atomic`.
- Manifest: `JJMMTT-HHMM_<Rechner>_Manifest.ferry` (JSON). Eindeutige Ordnernamen: `JJMMTT_<Rechner>_<Ordner>`.

## Oberfläche
- Feste Fensterbreite (420 px, `Metrics` in `ui/app.slint`), Höhe aus dem Inhalt; nichts darf das Fenster springen lassen. Lange Namen in der Mitte kürzen (`shorten` in `src/app.rs`).
- Schrift JetBrains Mono, Titel „MEDIAFILES FERRY“ in Helvetica Neue Light. Farben nur aus `Theme` (Kontrast im hellen Modus beachten). Standard: Dunkel.
- Alle Texte englisch in `@tr()`, Deutsch in `translations/de/…/mediafiles-ferry.po`; Anleitung in `docs/help/<sprache>.md`; neue Sprache zusätzlich in `LANGUAGES`.
- Menü liegt im gemeinsamen `FerryWindow` (macOS zeigt das Menü des aktiven Fensters); Aktionen aus dem Menü per `Timer::single_shot` verzögern (sonst Absturz beim Umbenennen der Einträge).
- Sichtprüfung ohne Bildschirmaufnahme: vorübergehend `window.take_snapshot()` nutzen und danach wieder entfernen.

## Dokumentation
`docs/Benutzerhandbuch.md` und `docs/help/*.md` bei jeder Funktionsänderung mitpflegen.

## Bauen / Testen
`cargo test`, `cargo build --release`, macOS-App: `scripts/build-mac-app.sh` → `dist/MediaFiles Ferry.app`.

# MediaFiles Ferry

Hält die Avid-Mediendateien (`Avid MediaFiles/MXF`) mehrerer Schnittplätze synchron – ohne dass die Systeme verbunden sind.

Ferry scannt die MXF-Ordner mit Prüfsummen, vergleicht die Bestände zweier Systeme über eine kleine Manifest-Datei, sammelt die fehlenden Dateien in einem Lieferordner und spielt diesen auf dem anderen System sicher ein. Dabei beachtet Ferry die Avid-Datenbanken (`msmFMID.pmr`, `msmMMOB.mdb`) und sammelt alles, was von Hand zu tun ist, in einer Aufgabenliste.

- löscht nie, überschreibt nie Mediendateien
- prüft jede Kopie per Prüfsumme (BLAKE3)
- funktioniert mit lokalen und Netzlaufwerken
- macOS zuerst, Windows folgt

**→ [Benutzerhandbuch](docs/Benutzerhandbuch.md)**

## Status

| Phase | Inhalt | Stand |
|---|---|---|
| 1 | Kern + Kommandozeile: Scan, Vergleich, Sammeln, Prüfen, Einspielen | fertig |
| 2 | Grafische Oberfläche (Slint), Englisch/Deutsch, Dark/Light | in Arbeit (Startbildschirm) |
| 3 | Manifest-Austausch per Peer-to-Peer (iroh) | geplant |
| 4 | Windows | geplant |

## Bauen

Voraussetzung: [Rust](https://rustup.rs)

```
cargo build --release
./target/release/ferry-app   # Oberfläche
./target/release/ferry       # Kommandozeile
cargo test
```

## Übersetzen

Die Oberfläche ist auf Englisch geschrieben (`@tr("…")` in `ui/app.slint`). Eine neue Sprache:

1. `translations/<code>/LC_MESSAGES/mediafiles-ferry.po` anlegen (Vorlage: `translations/de/…`, oder mit `slint-tr-extractor ui/*.slint` erzeugen)
2. die Sprache in `LANGUAGES` in `src/app.rs` eintragen

## Lizenz

GPL-3.0 oder neuer – siehe [LICENSE](LICENSE). Veränderte Versionen, die weitergegeben werden, müssen ebenfalls unter der GPL offengelegt werden.

*Avid und Media Composer sind Marken von Avid Technology, Inc. Dieses Projekt steht in keiner Verbindung zu Avid.*

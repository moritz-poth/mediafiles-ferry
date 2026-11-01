# MediaFiles Ferry – Benutzerhandbuch

MediaFiles Ferry hält die Avid-Mediendateien (MXF) mehrerer Schnittplätze auf demselben Stand, auch wenn die Systeme nicht miteinander verbunden sind. Ferry vergleicht die Bestände, sammelt die fehlenden Dateien in einem Lieferordner und spielt diesen auf dem anderen System sicher ein.

> **Stand:** Testversion mit Kommandozeile. Die grafische Oberfläche folgt; Abläufe und Regeln bleiben gleich.

---

## 1. Grundprinzip

```
 System A                                  System B
 ────────                                  ────────
 1. Scannen  → A.ferry.json  ──────────►   (Manifest von A)
   (Manifest von B)   ◄──────────────────  B.ferry.json ← 1. Scannen
 2. Sammeln  → Lieferordner  ──(Festplatte/Cloud)──►  3. Prüfen + Einspielen
```

1. **Scannen:** Ferry erfasst auf jedem System alle MXF-Dateien mit Prüfsumme und schreibt ein *Manifest* (eine kleine Datei mit der Liste der Dateien, keine Medien).
2. **Austauschen:** Die Manifeste werden zwischen den Systemen ausgetauscht (z. B. per Mail oder USB-Stick).
3. **Sammeln:** Jedes System sammelt die Dateien, die dem anderen fehlen, in einem Lieferordner.
4. **Prüfen und Einspielen:** Der Lieferordner wird auf das andere System gebracht, geprüft und eingespielt.

Ferry **löscht nie** Mediendateien und **überschreibt nie** eine vorhandene Mediendatei.

## 2. Was Ferry erwartet

Auf jedem Laufwerk (auch Netzlaufwerken) liegt:

```
<Laufwerk>/Avid MediaFiles/MXF/
    <Ordner>/            z. B. "1", "Avid8.1", "Mac Studio von Alex.1", "SFX 01 …"
        *.mxf            Mediendateien
        msmFMID.pmr      Avid-Datenbank
        msmMMOB.mdb      Avid-Datenbank
        Creating/        wird ignoriert
```

Ordnernamen sind beliebig. Weitere Unterordner (außer `Creating`) werden ignoriert und als Warnung gemeldet.

## 3. Schritt für Schritt

### 3.1 Scannen

Ferry findet alle angeschlossenen Laufwerke mit `Avid MediaFiles/MXF` und berechnet für jede MXF-Datei eine Prüfsumme.

- **Der erste Scan dauert lange**, weil alle Dateien einmal gelesen werden (je nach Laufwerk etwa 10 Minuten bis mehrere Stunden pro Terabyte-Bereich).
- **Weitere Scans sind schnell:** Ferry merkt sich die Prüfsummen und liest nur neue oder veränderte Dateien.
- **Vollständig prüfen:** liest alle Dateien neu. Hat sich der Inhalt einer Datei verändert, obwohl Größe und Datum gleich sind, meldet Ferry das als *vermutlich beschädigt*. Empfehlung: gelegentlich, z. B. vor Projektende.

Das Ergebnis ist eine Manifest-Datei, z. B. `MacStudio.ferry.json`. Diese Datei an das andere System schicken.

### 3.2 Vergleichen

Mit dem eigenen und dem fremden Manifest zeigt Ferry:

- wie viele Dateien (und GB) für das andere System gesammelt werden,
- Ordner, die auf beiden Seiten eigene Dateien haben (Entscheidung nötig, siehe 4.3),
- Laufwerke, die einander zugeordnet werden müssen (siehe 4.2),
- die Aufgabenliste (siehe 5).

### 3.3 Sammeln

Ferry kopiert die fehlenden Dateien in einen Ordner deiner Wahl, sortiert nach Laufwerk und Ordner:

```
<Lieferordner>/
    MEDIA_Data/
        Avid8.1/
            ….mxf
            msmFMID.pmr       (nur wenn die Datenbank mitgeliefert werden darf)
            msmMMOB.mdb
    Aufgaben.txt              Anleitung + Aufgabenliste
    ferry-lieferung.json      Prüfsummen der Lieferung
```

Jede Datei wird nach dem Kopieren nachgelesen und mit der Prüfsumme aus dem Scan verglichen. Wird das Sammeln unterbrochen, einfach neu starten: Vorhandene Dateien werden geprüft und übersprungen.

### 3.4 Lieferung prüfen

Nach dem Transport (Festplatte, Cloud, Kurier) auf dem Zielsystem **Lieferung prüfen** ausführen. Ferry liest alle Dateien und meldet fehlende oder beschädigte.

### 3.5 Lieferung einspielen

**Vorher Media Composer schließen.**

Ferry kopiert die Dateien in die richtigen Ordner unter `Avid MediaFiles/MXF/`:

- Fehlende Ordner werden angelegt, fehlende Dateien hinzugefügt.
- Vorhandene identische Dateien werden übersprungen; vorhandene Dateien mit anderem Inhalt werden **nicht** überschrieben, sondern gemeldet.
- Datenbankdateien werden nur ersetzt, wenn der Ordner danach exakt so aussieht wie auf dem Quellsystem. Hat sich der Ordner seit dem Scan verändert, bleibt die alte Datenbank und es entsteht die Aufgabe *Datenbank neu schreiben*.
- Ersetzte Datenbankdateien werden nicht gelöscht, sondern nach `Avid MediaFiles/Ferry-Backup/<Datum>/<Ordner>/` verschoben.

Laufwerke werden über ihren Namen gefunden. Heißt das Laufwerk auf diesem System anders, muss es zugeordnet werden.

### 3.6 Von Hand einspielen

Wer ohne Ferry einspielen möchte, findet die Anleitung auch in `Aufgaben.txt`:

1. Media Composer schließen.
2. Jeder Ordner der Lieferung heißt `<Laufwerk>/<Ordner>`. Auf dem Laufwerk den Ordner `Avid MediaFiles/MXF/<Ordner>` öffnen (fehlt er, neu anlegen).
3. **Nur den Inhalt des Lieferordners hineinkopieren – niemals den ganzen Ordner auf einen bestehenden Ordner ziehen!**
   Finder (Mac) und Explorer (Windows) bieten sonst „Ersetzen“ an – das löscht den vorhandenen Ordner samt allen Mediendateien.
4. Fragt das System, ob `.mxf`-Dateien ersetzt werden sollen: **nicht ersetzen**, abbrechen und prüfen.
5. `msmFMID.pmr` und `msmMMOB.mdb` nur in den Ordnern ersetzen, die in `Aufgaben.txt` aufgeführt sind.

## 4. Regeln

### 4.1 Wann wird was kopiert?

Eine Datei gilt als **vorhanden**, wenn sie irgendwo auf dem anderen System liegt – mit gleichem Namen und gleicher Prüfsumme, egal auf welchem Laufwerk oder in welchem Ordner.

| Situation | Was Ferry tut | Aufgabe |
|---|---|---|
| Ordner auf beiden Seiten identisch | nichts | – |
| Ordner fehlt auf der anderen Seite | ganzer Ordner mit Datenbank | – |
| Eine Seite hat zusätzliche Dateien | Dateien + Datenbank der volleren Seite | – |
| Beide Seiten haben eigene Dateien | nur die Dateien, keine Datenbank | Datenbank neu schreiben |
| Einige Dateien liegen drüben in einem anderen Ordner | nur die wirklich fehlenden Dateien, keine Datenbank | Datenbank neu schreiben |
| Gleicher Dateiname, anderer Inhalt | nicht kopiert | prüfen |

**Grundregel:** Die Datenbank wird nur mitgeliefert, wenn der Zielordner danach exakt dem Quellordner entspricht. Ferry löscht nie eine Datenbank.

### 4.2 Laufwerke zuordnen

Laufwerke mit gleichem Namen werden automatisch einander zugeordnet. Sonst fragt Ferry nach (z. B. wenn System A zwei Laufwerke nutzt, System B nur eins). Die Oberfläche wird sich die Zuordnung je Gegenstelle merken; in der Testversion wird sie mit `--map` angegeben.

### 4.3 Gleichnamige Ordner mit unterschiedlichem Inhalt

Avid legt auf jedem System eigene Ordner wie `1` oder `Rechnername.1` an. Gibt es einen Ordner auf beiden Seiten mit jeweils eigenen Dateien, fragt Ferry:

- **Zusammenführen:** Die Dateien kommen in denselben Ordner; danach muss die Datenbank neu geschrieben werden.
- **Eindeutig benennen:** Die Dateien kommen in einen neuen Ordner nach dem Muster `JJMMTT_Rechnername_Ordner`, z. B. `261031_MacStudio_1`. Danach muss die Datenbank dieses neuen Ordners geschrieben werden.

### 4.4 Groß-/Kleinschreibung und Umlaute

Mac und Windows speichern Namen unterschiedlich. Ferry vergleicht deshalb ohne Rücksicht auf Groß-/Kleinschreibung und Umlaut-Schreibweise: `Avid8.1` und `avid8.1` gelten als derselbe Ordner.

## 5. Die Aufgabenliste

Was Ferry nicht selbst erledigen darf, landet in der Aufgabenliste (auch in `Aufgaben.txt` jeder Lieferung, mit Kästchen zum Abhaken). In eckigen Klammern steht das System, auf dem die Aufgabe zu erledigen ist.

| Aufgabe | Bedeutung | Was tun |
|---|---|---|
| **Datenbank neu schreiben** | Im Ordner liegen Dateien, die die Datenbank nicht kennt. | Datenbank des Ordners in Media Composer neu aufbauen lassen. |
| **Ordner aufteilen (rebalancen)** | Der Ordner hat mehr als 5000 Dateien. Avid wird dann langsam. | Dateien auf mehrere Ordner verteilen. |
| **Gleicher Name, anderer Inhalt** | Zwei verschiedene Dateien heißen gleich (z. B. extern erzeugte MXF). | Prüfen, welche richtig ist. |
| **Doppelte Dateien** | Dieselbe Datei liegt in mehreren Ordnern. | Kein Fehler, kostet aber Platz. Bei Bedarf aufräumen. |
| **Nicht kopiert** | Beim Sammeln trat ein Fehler auf (z. B. Datei nicht lesbar). | Ursache beheben und erneut sammeln. |

## 6. Kommandozeile (Testversion)

Im Terminal im Projektordner:

| Befehl | Zweck |
|---|---|
| `ferry volumes` | gefundene Avid-Laufwerke anzeigen |
| `ferry scan [--full] [-o DATEI] [LAUFWERK…]` | scannen, Manifest schreiben |
| `ferry plan EIGENES.json ANDERES.json` | anzeigen, was fehlt |
| `ferry collect EIGENES.json ANDERES.json LIEFERORDNER` | sammeln |
| `ferry verify LIEFERORDNER` | Lieferung prüfen |
| `ferry import LIEFERORDNER` | Lieferung einspielen |

Optionen: `--map NAME=ZIEL` ordnet Laufwerke zu (bei `plan`/`collect`: eigenes Laufwerk = Laufwerk drüben; bei `import`: Laufwerk der Lieferung = Pfad, z. B. `--map MEDIA_Data=/Volumes/Projekt`). `--rename LAUFWERK/ORDNER` benennt einen Ordner drüben eindeutig statt zusammenzuführen.

## 7. Häufige Fragen

**Werden Mediendateien über das Internet übertragen?**
Nein. Ferry tauscht nur Manifeste aus. Die Medien reisen im Lieferordner auf dem Weg deiner Wahl.

**Kann ich Ferry abbrechen? Was passiert bei einem Absturz?**
Nichts geht verloren, und es muss nicht alles neu laufen:
- **Scannen:** Ferry sichert die berechneten Prüfsummen alle 30 Sekunden. Nach einem Absturz werden beim nächsten Scan nur die noch fehlenden Dateien gelesen.
- **Sammeln und Einspielen:** Halb kopierte Dateien heißen `….ferry-part` und gelten nie als fertig. Einfach neu starten: Fertige Dateien werden geprüft und übersprungen.
- Alle Verwaltungsdateien (Manifest, Prüfsummen, Lieferliste, Einstellungen) werden so geschrieben, dass ein Absturz nie eine halbe Datei hinterlässt.

**Warum löscht Ferry nichts?**
Ob eine Datei auf A gelöscht oder auf B neu ist, lässt sich ohne Verlauf nicht unterscheiden. Löschen bleibt deshalb Handarbeit.

**Wo speichert Ferry die Prüfsummen?**
Mac: `~/Library/Application Support/MediaFiles Ferry/`, Windows: `%APPDATA%\MediaFiles Ferry\`. Löschen ist unkritisch; der nächste Scan liest dann alles neu.

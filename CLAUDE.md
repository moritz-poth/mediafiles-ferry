# MediaFiles Ferry – notes for working on the project

Rust core + command line `ferry`, Slint app `ferry-app`. macOS first (universal arm64 + x86_64), Windows later. GPL-3.0. Users are editors and assistants without a technical background: simple, minimal, maximum data safety.

## Fixed rules
- Ferry never deletes media files and never overwrites an existing media file. Deleting stays manual.
- A database (`msmFMID.pmr`, `msmMMOB.mdb`) is never deleted and only delivered/replaced if the target folder ends up exactly like the source folder; otherwise task "rebuild database". Replaced databases go to `Avid MediaFiles/Ferry-Backup/`.
- A file counts as present if it exists anywhere on the other side with the same name and hash. Same name + different hash: don't copy, add a task.
- Everything the user must do by hand goes into the task list (`Tasks.txt` / `Aufgaben.txt` in the delivery, in the sender's language). The instructions for importing by hand ("only copy the contents, never replace the folder") are always in it.
- Import verifies the whole delivery first and only imports a complete one.
- Compare names via `key()` (NFC + lowercase). Write admin files via `write_atomic`.
- Manifest: `YYMMDD-HHMM_<computer>_Manifest.ferry` (JSON). Delivery: `ferry-delivery.json` (old name `ferry-lieferung.json` is still read). Unique folder names: `YYMMDD_<computer>_<folder>`.

## UI
- Fixed window width (420 px, `Metrics` in `ui/app.slint`), height from content. Wrapped text needs a fixed width (`Para`), otherwise Slint counts it as one line and the window is too short. Long pages use `ScrollArea`.
- Pages in one window (`Page`: home, compare, working, result, delivery); flow state in `src/flow.rs` (`Compare`, `Incoming`, remembered `Mappings`).
- Font JetBrains Mono, title "MEDIAFILES FERRY" in Helvetica Neue Light. Colors only from `Theme`. Default: dark. Long names are shortened in the middle (`shorten`).
- All UI text in English in `@tr()`, German in `translations/de/…/mediafiles-ferry.po`; texts Rust needs come from `out property txt-…` in the window. Core messages are English; tasks via `Task::describe(lang)`.
- Menus live in the shared `FerryWindow` (macOS shows the menu of the active window); run menu actions via `Timer::single_shot` (renaming entries during macOS menu handling crashed).
- Visual check without screen recording: temporarily use `window.take_snapshot()` in a hook, then remove it again.

## Icon
`assets/AppIcon.iconset` is designed per size on Apple's grid (16 px → 14 px plate …); sources `assets/MediaFilesFerry_*.svg`. Don't scale down from 1024.

## Docs
Update `docs/help/*.md` (in-app manual) and README with every feature change.

## Build / test
`cargo test`, `scripts/build-mac-app.sh` → universal `dist/MediaFiles Ferry.app` + zip (ad-hoc signed). Release: bump `version` in Cargo.toml and `BUILD` in the build script.
A two-system test environment lives outside the repo (built from real Avid folders); scan its volume folders directly with `ferry scan -o … <folder>…` and use a separate `HOME` so the hash cache stays apart.

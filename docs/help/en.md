# User Guide

## What MediaFiles Ferry does

MediaFiles Ferry keeps the Avid media files (MXF) of several edit systems in sync, even when the systems are not connected. Ferry compares what each system has, collects the missing files into a delivery folder and safely imports that folder on the other system.

Ferry **never deletes** media files and **never overwrites** an existing media file.

*Preview version:* comparing, collecting and importing are coming to the app next. Until then they are available in the command-line version.

## How it works

1. **Scan** each system. Ferry records every MXF file with a checksum and writes a small *manifest* file – a list of files, no media.
2. **Exchange** the manifests between the systems, e.g. by e-mail or USB stick.
3. **Collect:** each system collects the files the other one is missing into a delivery folder.
4. **Verify & import** the delivery folder on the other system.

## What Ferry expects

On every drive, including network drives:

- `Avid MediaFiles/MXF/<folder>/` with the `.mxf` media files
- the two Avid database files `msmFMID.pmr` and `msmMMOB.mdb` in each folder
- `Creating` folders are ignored

Folder names can be anything, e.g. `1`, `Avid8.1` or `Mac Studio.1`.

## Scan

Click **Scan**. Ferry finds all connected drives with `Avid MediaFiles/MXF` and computes a checksum for every MXF file.

- **The first scan takes a while**, because every file is read once.
- **Later scans are fast:** Ferry remembers the checksums and only reads new or changed files.
- If the app is closed or crashes during a scan, nothing is lost – the next scan continues where it stopped.

Then click **Save manifest…** and send the file to the other system. The file name contains date, time and computer name, e.g. `261101-1312_Mac Studio_Manifest.ferry`.

## When is something copied?

A file counts as **present** if it exists anywhere on the other system with the same name and checksum – on any drive, in any folder.

- **Folder identical on both sides:** nothing happens.
- **Folder missing on the other side:** the whole folder is copied, including its database.
- **One side has additional files:** the files and the database of the fuller side are copied.
- **Both sides have files of their own:** only the files are copied, no database – a task “rebuild database” is added.
- **Same file name, different content:** not copied – a task asks you to check it.

**Rule of thumb:** a database is only delivered if the target folder ends up exactly like the source folder. Ferry never deletes a database.

## Drives and folders with the same name

Drives with the same name are matched automatically; otherwise Ferry asks you to assign them.

Avid creates folders like `1` on every system. If such a folder exists on both sides with different files, you choose:

- **Merge:** the files go into the same folder; the database must be rebuilt afterwards.
- **Rename:** the files go into a new folder like `261031_MacStudio_1`.

## Task list

Everything Ferry must not do itself ends up in the task list (also in `Aufgaben.txt` of each delivery). The system on which the task has to be done is shown in brackets.

- **Rebuild database** – the folder contains files the database does not know. Let Media Composer rebuild it.
- **Rebalance folder** – the folder has more than 5000 files; Avid gets slow. Spread the files over several folders.
- **Same name, different content** – two different files share a name. Check which one is right.
- **Duplicate files** – the same file exists in several folders. Not an error, but it costs space.

## Importing by hand

1. Quit Media Composer.
2. For each folder of the delivery, open `Avid MediaFiles/MXF/<folder>` on the drive (create it if missing).
3. **Copy only the contents of the delivery folder – never drag the whole folder onto an existing folder!** Finder and Explorer would replace it including all media files.
4. If you are asked to replace `.mxf` files: **don’t** – cancel and check.
5. Replace `msmFMID.pmr` and `msmMMOB.mdb` only in the folders listed in `Aufgaben.txt`.

## Settings

In the **Settings** menu you can choose the language and the appearance (dark or light).

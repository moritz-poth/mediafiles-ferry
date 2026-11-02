# Manual

## What MediaFiles Ferry does

MediaFiles Ferry keeps the Avid media files (MXF) of several edit systems in sync, even when the systems are not connected. Ferry compares what each system has, collects the missing files into a delivery folder and safely imports that folder on the other system.

Ferry **never deletes** media files and **never overwrites** an existing media file.

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

## Compare and collect

1. Scan this system first, so that your own manifest is up to date.
2. Click **Open manifest…** and choose the manifest of the other system.
3. **Assign drives:** drives without a drive of the same name on the other side need a target – or **Don't send**. Ferry remembers your choice for this system.
4. **Folders on both sides:** choose **Merge** or **Rename** for every folder (see below).
5. Check the number of files and the tasks, then click **Collect…** and choose where the delivery folder should be created – e.g. on an external drive.

Ferry copies every file with a checksum check into a new folder named like `261102-1830_Mac Studio_to_MacBook`. It also contains `Tasks.txt` with instructions for importing by hand. If collecting is interrupted, collecting again continues where it stopped.

## Verify and import

1. Quit Media Composer.
2. Click **Verify & import…** and choose the delivery folder.
3. Check the drives: every drive of the delivery needs a drive on this system. Drives with the same name are assigned automatically.
4. Click **Verify & import**. Ferry first verifies every file of the delivery. Only if everything is complete and undamaged does it import – it only adds files and never overwrites a media file. Replaced databases are kept in `Avid MediaFiles/Ferry-Backup/`.
5. Scan again afterwards, so that your manifest is up to date.

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

Everything Ferry must not do itself ends up in the task list (also in `Tasks.txt` of each delivery). The system on which the task has to be done is shown in brackets.

- **Rebuild database** – the folder contains files the database does not know. Let Media Composer rebuild it.
- **Rebalance folder** – the folder has more than 5000 files; Avid gets slow. Spread the files over several folders.
- **Same name, different content** – two different files share a name. Check which one is right.
- **Duplicate files** – the same file exists in several folders. Not an error, but it costs space.

## Importing by hand

1. Quit Media Composer.
2. For each folder of the delivery, open `Avid MediaFiles/MXF/<folder>` on the drive (create it if missing).
3. **Copy only the contents of the delivery folder – never drag the whole folder onto an existing folder!** Finder and Explorer would replace it including all media files.
4. If you are asked to replace `.mxf` files: **don’t** – cancel and check.
5. Replace `msmFMID.pmr` and `msmMMOB.mdb` only in the folders listed in `Tasks.txt` (German: `Aufgaben.txt`).

## Settings

In the **Settings** menu you can choose the language and the appearance (dark or light). **Help → About MediaFiles Ferry** shows the version and the license.

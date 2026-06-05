use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};

use crate::models::{LocalLibrary, LocalScanReport, LocalTrack, stable_local_track_id};

const SUPPORTED_AUDIO_EXTENSIONS: [&str; 6] = ["mp3", "wav", "flac", "ogg", "m4a", "aac"];

pub fn scan_local_library(root: &Path, previous: &LocalLibrary) -> Result<(LocalLibrary, LocalScanReport)> {
    if !root.is_absolute() {
        anyhow::bail!("local music path must be absolute");
    }
    if !root.is_dir() {
        anyhow::bail!("local music path must be an existing directory");
    }

    let mut paths = Vec::new();
    let mut skipped = 0usize;
    collect_supported_files(root, &mut paths, &mut skipped)?;
    paths.sort();

    let previous_by_id: HashMap<String, LocalTrack> = previous
        .tracks
        .iter()
        .map(|track| (track.id.clone(), track.clone()))
        .collect();

    let mut seen_ids = HashSet::new();
    let mut tracks = Vec::new();
    let mut report = LocalScanReport {
        files_found: paths.len(),
        skipped,
        ..LocalScanReport::default()
    };

    for path in paths {
        let id = stable_local_track_id(&path);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                report.skipped += 1;
                continue;
            }
        };
        let file_size = metadata.len();
        let modified_unix_secs = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or_default();

        let track = if let Some(previous_track) = previous_by_id.get(&id) {
            if previous_track.file_size == file_size
                && previous_track.modified_unix_secs == modified_unix_secs
            {
                previous_track.clone()
            } else {
                report.tracks_updated += 1;
                fallback_track_from_path(id.clone(), path, file_size, modified_unix_secs)
            }
        } else {
            report.tracks_added += 1;
            fallback_track_from_path(id.clone(), path, file_size, modified_unix_secs)
        };

        seen_ids.insert(id);
        tracks.push(track);
    }

    report.tracks_removed = previous
        .tracks
        .iter()
        .filter(|track| !seen_ids.contains(&track.id))
        .count();

    Ok((LocalLibrary { tracks }, report))
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_AUDIO_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or(false)
}

fn collect_supported_files(
    dir: &Path,
    paths: &mut Vec<PathBuf>,
    skipped: &mut usize,
) -> Result<()> {
    let entries = fs::read_dir(dir).with_context(|| format!("unable to read {}", dir.display()))?;

    for entry in entries {
        let Ok(entry) = entry else {
            *skipped += 1;
            continue;
        };
        let path = entry.path();
        if path.is_dir() {
            if collect_supported_files(&path, paths, skipped).is_err() {
                *skipped += 1;
            }
        } else if is_supported_audio_file(&path) {
            paths.push(path);
        } else {
            *skipped += 1;
        }
    }

    Ok(())
}

fn fallback_track_from_path(
    id: String,
    path: PathBuf,
    file_size: u64,
    modified_unix_secs: u64,
) -> LocalTrack {
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Unknown Track")
        .to_string();

    LocalTrack {
        id,
        path,
        title,
        artist: String::new(),
        album: String::new(),
        duration_ms: 0,
        artwork_path: None,
        file_size,
        modified_unix_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("echo-local-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn supported_extensions_are_case_insensitive() {
        assert!(is_supported_audio_file(Path::new("track.MP3")));
        assert!(is_supported_audio_file(Path::new("track.FlAc")));
        assert!(!is_supported_audio_file(Path::new("track.txt")));
    }

    #[test]
    fn recursive_scan_finds_supported_nested_files() {
        let root = unique_temp_dir("recursive");
        let nested = root.join("Artist").join("Album");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(root.join("root.mp3"), b"root").expect("write root file");
        fs::write(nested.join("nested.FLAC"), b"nested").expect("write nested file");
        fs::write(nested.join("notes.txt"), b"notes").expect("write unsupported file");

        let (library, report) =
            scan_local_library(&root, &LocalLibrary::default()).expect("scan local library");

        assert_eq!(library.tracks.len(), 2);
        assert_eq!(report.files_found, 2);
        assert_eq!(report.tracks_added, 2);
        assert_eq!(report.skipped, 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_reports_unchanged_updated_added_and_removed_files() {
        let root = unique_temp_dir("fingerprints");
        fs::create_dir_all(&root).expect("create root");
        let keep = root.join("keep.mp3");
        let update = root.join("update.mp3");
        let added = root.join("added.mp3");
        fs::write(&keep, b"keep").expect("write keep");
        fs::write(&update, b"update").expect("write update");

        let (mut previous, _) =
            scan_local_library(&root, &LocalLibrary::default()).expect("initial scan");
        let removed_id = "local:removed".to_string();
        previous.tracks.push(LocalTrack {
            id: removed_id,
            path: root.join("removed.mp3"),
            title: "Removed".to_string(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
            artwork_path: None,
            file_size: 1,
            modified_unix_secs: 1,
        });

        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&update, b"updated").expect("rewrite update");
        fs::write(&added, b"added").expect("write added");

        let (library, report) = scan_local_library(&root, &previous).expect("rescan");

        assert_eq!(library.tracks.len(), 3);
        assert_eq!(report.tracks_added, 1);
        assert_eq!(report.tracks_updated, 1);
        assert_eq!(report.tracks_removed, 1);

        let _ = fs::remove_dir_all(root);
    }
}

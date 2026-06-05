use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    picture::MimeType,
    prelude::Accessor,
};

use crate::config::AppConfig;
use crate::models::{LocalLibrary, LocalScanReport, LocalTrack, stable_local_track_id};

const SUPPORTED_AUDIO_EXTENSIONS: [&str; 6] = ["mp3", "wav", "flac", "ogg", "m4a", "aac"];

pub fn scan_local_library(
    root: &Path,
    previous: &LocalLibrary,
) -> Result<(LocalLibrary, LocalScanReport)> {
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
                match track_from_path(id.clone(), path, file_size, modified_unix_secs) {
                    Ok(track) => track,
                    Err(_) => {
                        report.skipped += 1;
                        continue;
                    }
                }
            }
        } else {
            report.tracks_added += 1;
            match track_from_path(id.clone(), path, file_size, modified_unix_secs) {
                Ok(track) => track,
                Err(_) => {
                    report.skipped += 1;
                    continue;
                }
            }
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

fn track_from_path(
    id: String,
    path: PathBuf,
    file_size: u64,
    modified_unix_secs: u64,
) -> Result<LocalTrack> {
    parse_track_metadata(&id, &path, file_size, modified_unix_secs)
}

fn parse_track_metadata(
    id: &str,
    path: &Path,
    file_size: u64,
    modified_unix_secs: u64,
) -> Result<LocalTrack> {
    let tagged_file = lofty::read_from_path(path)?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    let fallback = fallback_track_title(path);
    let title = tag
        .and_then(|tag| tag.title())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or(fallback);
    let artist = tag
        .and_then(|tag| tag.artist())
        .map(|artist| artist.trim().to_string())
        .unwrap_or_default();
    let album = tag
        .and_then(|tag| tag.album())
        .map(|album| album.trim().to_string())
        .unwrap_or_default();
    let duration_ms = tagged_file
        .properties()
        .duration()
        .as_millis()
        .try_into()
        .unwrap_or(u32::MAX);
    let artwork_path =
        tag.and_then(|tag| write_embedded_artwork(id, tag.pictures()).ok().flatten());

    Ok(LocalTrack {
        id: id.to_string(),
        path: path.to_path_buf(),
        title,
        artist,
        album,
        duration_ms,
        artwork_path,
        file_size,
        modified_unix_secs,
    })
}

fn fallback_track_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Unknown Track")
        .to_string()
}

fn write_embedded_artwork(
    track_id: &str,
    pictures: &[lofty::picture::Picture],
) -> Result<Option<PathBuf>> {
    let Some(picture) = pictures.first() else {
        return Ok(None);
    };
    if picture.data().is_empty() {
        return Ok(None);
    }

    let ext = picture.mime_type().and_then(MimeType::ext).unwrap_or("img");
    let safe_track_id = track_id.replace([':', '/', '\\'], "_");
    let dir = AppConfig::local_artwork_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{safe_track_id}.{ext}"));
    fs::write(&path, picture.data())?;
    Ok(Some(path))
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

    fn write_silent_wav(path: &Path, sample_count: u32) {
        let channels = 1u16;
        let sample_rate = 8_000u32;
        let bits_per_sample = 16u16;
        let block_align = channels * bits_per_sample / 8;
        let byte_rate = sample_rate * u32::from(block_align);
        let data_size = sample_count * u32::from(block_align);
        let riff_size = 36 + data_size;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&riff_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.resize(bytes.len() + data_size as usize, 0);
        fs::write(path, bytes).expect("write wav file");
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
        write_silent_wav(&root.join("root.wav"), 8_000);
        write_silent_wav(&nested.join("nested.WAV"), 4_000);
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
        let keep = root.join("keep.wav");
        let update = root.join("update.wav");
        let added = root.join("added.wav");
        write_silent_wav(&keep, 8_000);
        write_silent_wav(&update, 8_000);

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
        write_silent_wav(&update, 16_000);
        write_silent_wav(&added, 8_000);

        let (library, report) = scan_local_library(&root, &previous).expect("rescan");

        assert_eq!(library.tracks.len(), 3);
        assert_eq!(report.tracks_added, 1);
        assert_eq!(report.tracks_updated, 1);
        assert_eq!(report.tracks_removed, 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_skips_supported_files_that_cannot_be_decoded() {
        let root = unique_temp_dir("undecodable");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("not-a-song.mp3"), b"nope").expect("write invalid audio");

        let (library, report) =
            scan_local_library(&root, &LocalLibrary::default()).expect("scan local library");

        assert!(library.tracks.is_empty());
        assert_eq!(report.files_found, 1);
        assert_eq!(report.skipped, 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn decoded_files_use_filename_fallback_and_duration() {
        let root = unique_temp_dir("metadata-fallback");
        fs::create_dir_all(&root).expect("create root");
        write_silent_wav(&root.join("fallback-title.wav"), 8_000);

        let (library, _) =
            scan_local_library(&root, &LocalLibrary::default()).expect("scan local library");

        let track = library.tracks.first().expect("decoded track");
        assert_eq!(track.title, "fallback-title");
        assert_eq!(track.artist, "");
        assert_eq!(track.album, "");
        assert_eq!(track.duration_ms, 1000);

        let _ = fs::remove_dir_all(root);
    }
}

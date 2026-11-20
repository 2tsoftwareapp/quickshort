//! Behaviour of the library index: version discipline, annotation carry-forward,
//! query composition, and the capture-path upsert.

use qs_library::entry::{CaptureMode, EntryKind};
use qs_library::{
    index, is_supported, query, scan, DateFilter, FilterState, IndexState, LibraryEntry,
    LibraryIndex, Provenance, Sort, INDEX_FILE, INDEX_VERSION,
};
use std::path::Path;

fn entry(name: &str, mtime: i64, size: u64, tags: &[&str]) -> LibraryEntry {
    LibraryEntry {
        file_name: name.to_string(),
        kind: EntryKind::Image,
        size_bytes: size,
        modified_unix: mtime,
        width: 100,
        height: 50,
        mode: CaptureMode::Unknown,
        source_app: None,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        favorite: false,
    }
}

fn write_png(path: &Path) {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
    img.save(path).unwrap();
}

#[test]
fn index_round_trips_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut idx = LibraryIndex::default();
    let mut e = entry("a.png", 1_700_000_000, 10, &["invoice"]);
    e.favorite = true;
    e.mode = CaptureMode::Region;
    e.source_app = Some("Safari".into());
    idx.entries.insert(e.file_name.clone(), e);

    index::save(dir.path(), &idx).unwrap();
    assert!(!dir.path().join(format!("{INDEX_FILE}.tmp")).exists());

    match index::load(dir.path()) {
        IndexState::Ready(loaded) => {
            let got = loaded.entries.get("a.png").expect("entry");
            assert_eq!(got.tags, vec!["invoice".to_string()]);
            assert!(got.favorite);
            assert_eq!(got.mode, CaptureMode::Region);
            assert_eq!(got.source_app.as_deref(), Some("Safari"));
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn missing_index_is_a_rebuild_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    match index::load(dir.path()) {
        IndexState::Rebuild { notice } => {
            assert!(notice.is_empty(), "no notice for a fresh folder")
        }
        other => panic!("expected Rebuild, got {other:?}"),
    }
}

#[test]
fn index_from_a_newer_build_is_never_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(INDEX_FILE);
    std::fs::write(&path, r#"{"version":99,"entries":{}}"#).unwrap();

    match index::load(dir.path()) {
        IndexState::TooNew { found, notice } => {
            assert_eq!(found, 99);
            assert!(notice.contains("newer quickshort"));
        }
        other => panic!("expected TooNew, got {other:?}"),
    }

    // The capture path must refuse rather than clobber the newer file.
    let img = dir.path().join("shot.png");
    write_png(&img);
    assert!(qs_library::record_capture(
        dir.path(),
        &img,
        Provenance {
            mode: CaptureMode::Region,
            source_app: None
        }
    )
    .is_err());
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("\"version\":99"),
        "newer index was overwritten"
    );
}

#[test]
fn corrupt_index_rebuilds_and_reports_the_loss() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(INDEX_FILE), "{not json").unwrap();
    match index::load(dir.path()) {
        IndexState::Rebuild { notice } => assert!(notice.contains("could not be recovered")),
        other => panic!("expected Rebuild, got {other:?}"),
    }
}

#[test]
fn merge_keeps_annotations_for_unchanged_files_only() {
    let mut idx = LibraryIndex::default();
    idx.entries
        .insert("a.png".into(), entry("a.png", 1_700_000_000, 10, &["keep"]));
    idx.entries.insert(
        "b.png".into(),
        entry("b.png", 1_700_000_000, 10, &["stale"]),
    );

    // a.png unchanged, b.png rewritten (different size/mtime), c.png new.
    let scanned = vec![
        entry("a.png", 1_700_000_000, 10, &[]),
        entry("b.png", 1_700_000_500, 99, &[]),
        entry("c.png", 1_700_000_000, 10, &[]),
    ];
    let merged = index::merge(Some(&idx), scanned);

    assert_eq!(merged.entries["a.png"].tags, vec!["keep".to_string()]);
    assert!(
        merged.entries["b.png"].tags.is_empty(),
        "stale tags resurrected"
    );
    assert!(merged.entries["c.png"].tags.is_empty());
    assert_eq!(merged.version, INDEX_VERSION);
}

#[test]
fn merge_prunes_entries_whose_files_are_gone() {
    let mut idx = LibraryIndex::default();
    idx.entries
        .insert("gone.png".into(), entry("gone.png", 1, 1, &["x"]));
    let merged = index::merge(Some(&idx), vec![]);
    assert!(merged.entries.is_empty());
}

#[test]
fn filters_compose_and_sorts_are_stable() {
    let mut a = entry("billing.png", 1_700_000_000, 1, &["invoice"]);
    let mut b = entry("dashboard.png", 1_700_100_000, 1, &[]);
    let c = entry("notes.qsdoc", 1_700_200_000, 1, &[]);
    a.favorite = true;
    b.kind = EntryKind::Image;
    let mut c = c;
    c.kind = EntryKind::Document;
    let all = vec![a, b, c];

    // Newest first by default.
    let newest = FilterState::default().apply(&all, 1_700_300_000);
    assert_eq!(newest[0].file_name, "notes.qsdoc");

    // Search matches name or tag, case-insensitively.
    let mut f = FilterState {
        search: "INVOICE".into(),
        ..Default::default()
    };
    assert_eq!(f.apply(&all, 1_700_300_000).len(), 1);

    // Kind filter composes with search and tag.
    f.kind = Some(EntryKind::Document);
    assert!(f.apply(&all, 1_700_300_000).is_empty());
    f.kind = None;
    f.tag = Some("invoice".into());
    assert_eq!(f.apply(&all, 1_700_300_000).len(), 1);

    // Favourites only.
    let fav = FilterState {
        favorites_only: true,
        ..Default::default()
    };
    assert_eq!(fav.apply(&all, 1_700_300_000)[0].file_name, "billing.png");

    // Name sort is case-insensitive.
    let by_name = FilterState {
        sort: Sort::Name,
        ..Default::default()
    };
    let names: Vec<&str> = by_name
        .apply(&all, 1_700_300_000)
        .iter()
        .map(|e| e.file_name.as_str())
        .collect();
    assert_eq!(names, vec!["billing.png", "dashboard.png", "notes.qsdoc"]);
}

#[test]
fn date_filter_uses_whole_utc_days() {
    let now = 1_700_000_000;
    let all = vec![
        entry("today.png", now - 60, 1, &[]),
        entry("six_days.png", now - 6 * 86_400, 1, &[]),
        entry("eight_days.png", now - 8 * 86_400, 1, &[]),
    ];
    let f = |d| FilterState {
        date: d,
        ..Default::default()
    };
    assert_eq!(f(DateFilter::All).apply(&all, now).len(), 3);
    assert_eq!(f(DateFilter::Today).apply(&all, now).len(), 1);
    assert_eq!(f(DateFilter::Last7Days).apply(&all, now).len(), 2);
    assert_eq!(f(DateFilter::Last30Days).apply(&all, now).len(), 3);
}

#[test]
fn dates_format_as_calendar_days() {
    assert_eq!(query::format_date(0), "—");
    assert_eq!(query::format_date(1_700_000_000), "2023-11-14");
    // A leap day, to pin the civil-date arithmetic.
    assert_eq!(query::format_date(1_709_164_800), "2024-02-29");
}

#[test]
fn supported_extensions_are_the_documented_ones() {
    for ok in ["a.png", "b.JPG", "c.jpeg", "d.webp", "e.qsdoc"] {
        assert!(is_supported(Path::new(ok)), "{ok} should be supported");
    }
    for bad in ["notes.txt", ".DS_Store", "shot.png.part", "noext"] {
        assert!(!is_supported(Path::new(bad)), "{bad} should be ignored");
    }
}

#[test]
fn scan_reads_dimensions_and_ignores_junk() {
    let dir = tempfile::tempdir().unwrap();
    write_png(&dir.path().join("shot.png"));
    std::fs::write(dir.path().join(".DS_Store"), b"junk").unwrap();
    std::fs::write(dir.path().join(INDEX_FILE), b"{}").unwrap();

    let found = scan::read_dir(dir.path()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file_name, "shot.png");
    assert_eq!((found[0].width, found[0].height), (2, 2));
}

#[test]
fn missing_folder_scans_empty() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("not-there");
    assert!(scan::read_dir(&missing).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn unreadable_folder_reports_the_reason() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    // A root shell can read it anyway; only assert when the permission bites.
    let err = scan::read_dir(&locked).err();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    if let Some(err) = err {
        let msg = format!("{err:#}");
        assert!(
            msg.contains("reading capture folder"),
            "unhelpful error: {msg}"
        );
    }
}

#[test]
fn record_capture_upserts_without_a_rescan() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("quickshort-1700000000.png");
    write_png(&img);

    qs_library::record_capture(
        dir.path(),
        &img,
        Provenance {
            mode: CaptureMode::Scrolling,
            source_app: Some("Slack".into()),
        },
    )
    .unwrap();

    match index::load(dir.path()) {
        IndexState::Ready(idx) => {
            let e = idx.entries.get("quickshort-1700000000.png").unwrap();
            assert_eq!(e.mode, CaptureMode::Scrolling);
            assert_eq!(e.source_app.as_deref(), Some("Slack"));
            assert_eq!((e.width, e.height), (2, 2));
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn tags_are_unique_case_insensitively() {
    let mut e = entry("a.png", 1, 1, &["Invoice"]);
    assert!(!e.add_tag("invoice"), "duplicate tag accepted");
    assert!(e.add_tag("paid"));
    assert!(e.has_tag("INVOICE"));
    assert!(e.remove_tag("invoice"));
    assert_eq!(e.tags, vec!["paid".to_string()]);
    assert!(!e.add_tag("   "), "blank tag accepted");
}

#[test]
fn tag_list_is_deduplicated_and_sorted() {
    let a = entry("a.png", 1, 1, &["Zeta", "alpha"]);
    let b = entry("b.png", 1, 1, &["ALPHA", "beta"]);
    let tags = index::all_tags([&a, &b]);
    assert_eq!(
        tags,
        vec!["alpha".to_string(), "beta".into(), "Zeta".into()]
    );
}

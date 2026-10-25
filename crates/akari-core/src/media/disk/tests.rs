use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::cache::tests::TempDir;

const KEY: &str = "avatar/80351110224678912/0123456789abcdef0123456789abcdef";

static NEXT: AtomicU64 = AtomicU64::new(0);

async fn download(tier: &DiskTier, bytes: usize) -> PathBuf {
    let temporary = tier
        .temporary()
        .join(format!("download-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::create_dir_all(tier.temporary()).unwrap();
    fs::write(&temporary, vec![7; bytes]).unwrap();
    temporary
}

async fn keep(
    tier: &DiskTier,
    key: &str,
    form: Form,
    side: u32,
    full: bool,
    bytes: usize,
) -> Entry {
    let temporary = download(tier, bytes).await;
    tier.insert(NewFile {
        key: key.to_owned(),
        form,
        width: side,
        height: side,
        animated: form != Form::Still,
        full,
        extension: "webp",
        bytes: bytes as u64,
        temporary: temporary.clone(),
    })
    .await
    .unwrap()
}

fn tier(dir: &TempDir, limit: u64) -> DiskTier {
    DiskTier::start(dir.path().join("1234"), limit)
}

#[tokio::test]
async fn a_kept_file_is_found_and_its_download_moved() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);

    let kept = keep(&tier, KEY, Form::Still, 80, false, 100).await;
    let found = tier.find(KEY, Form::Still, 80).await.unwrap();

    assert_eq!(found, kept);
    assert_eq!(fs::read(&found.path).unwrap(), vec![7; 100]);
    assert!(found.path.starts_with(dir.path().join("1234")));
    assert_eq!(
        fs::read_dir(tier.temporary()).unwrap().count(),
        0,
        "the download moved"
    );
    assert_eq!(tier.find("avatar/other", Form::Still, 80).await, None);
}

#[tokio::test]
async fn a_larger_or_full_file_answers_a_smaller_request() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);
    keep(&tier, KEY, Form::Still, 80, false, 100).await;
    keep(&tier, KEY, Form::Still, 160, false, 200).await;
    keep(&tier, "emoji/1", Form::Still, 64, true, 100).await;

    assert_eq!(tier.find(KEY, Form::Still, 40).await.unwrap().width, 80);
    assert_eq!(tier.find(KEY, Form::Still, 96).await.unwrap().width, 160);
    assert_eq!(tier.find(KEY, Form::Still, 320).await, None);
    assert_eq!(
        tier.find(KEY, Form::WebP, 40).await,
        None,
        "animated is another file"
    );
    assert_eq!(
        tier.find("emoji/1", Form::Still, 512).await.unwrap().width,
        64,
        "the source's own size answers any size"
    );
}

#[tokio::test]
async fn files_survive_a_restart() {
    let dir = TempDir::new();
    let first = tier(&dir, LIMIT);
    keep(&first, KEY, Form::Gif, 80, false, 100).await;
    first.close().await;

    let second = tier(&dir, LIMIT);

    let found = second.find(KEY, Form::Gif, 80).await.unwrap();
    assert!(found.animated);
    assert_eq!(second.total().await, 100);
}

#[tokio::test]
async fn eviction_drops_the_least_recently_used_down_to_ninety_percent() {
    let dir = TempDir::new();
    let tier = tier(&dir, 1000);
    tier.set_clock(1_000);
    let first = keep(&tier, "a", Form::Still, 80, false, 300).await;
    tier.set_clock(1_100);
    let second = keep(&tier, "b", Form::Still, 80, false, 300).await;
    tier.set_clock(1_200);
    let third = keep(&tier, "c", Form::Still, 80, false, 300).await;
    tier.set_clock(1_300);
    tier.find("a", Form::Still, 80).await.unwrap();

    tier.set_clock(1_400);
    keep(&tier, "d", Form::Still, 80, false, 300).await;

    assert!(tier.total().await <= 900);
    assert_eq!(
        tier.find("b", Form::Still, 80).await,
        None,
        "least recently used"
    );
    assert!(!second.path.exists());
    assert!(tier.find("a", Form::Still, 80).await.is_some());
    assert!(first.path.exists() && third.path.exists());
}

#[tokio::test]
async fn nothing_used_in_the_last_minute_is_evicted() {
    let dir = TempDir::new();
    let tier = tier(&dir, 1000);
    tier.set_clock(1_000);
    keep(&tier, "a", Form::Still, 80, false, 600).await;
    tier.set_clock(1_030);

    keep(&tier, "b", Form::Still, 80, false, 600).await;

    assert!(tier.find("a", Form::Still, 80).await.is_some());
    assert!(tier.find("b", Form::Still, 80).await.is_some());
    assert_eq!(
        tier.total().await,
        1200,
        "over the limit until the minute passes"
    );
    tier.set_clock(1_200);
    keep(&tier, "c", Form::Still, 80, false, 100).await;
    assert!(tier.total().await <= 900);
}

#[tokio::test]
async fn a_replaced_file_counts_once() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);

    keep(&tier, KEY, Form::Still, 80, false, 100).await;
    keep(&tier, KEY, Form::Still, 80, false, 150).await;

    assert_eq!(tier.total().await, 150);
    assert_eq!(
        fs::read(tier.find(KEY, Form::Still, 80).await.unwrap().path)
            .unwrap()
            .len(),
        150
    );
}

#[tokio::test]
async fn a_broken_index_starts_over() {
    let dir = TempDir::new();
    let first = tier(&dir, LIMIT);
    let kept = keep(&first, KEY, Form::Still, 80, false, 100).await;
    first.close().await;
    let index = dir.path().join("1234").join(INDEX);
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(dir.path().join("1234").join(format!("{INDEX}{suffix}")));
    }
    fs::write(&index, b"not a database, not at all").unwrap();

    let second = tier(&dir, LIMIT);

    assert_eq!(second.find(KEY, Form::Still, 80).await, None);
    assert!(!kept.path.exists(), "the old files go with the index");
    assert!(
        keep(&second, KEY, Form::Still, 80, false, 100)
            .await
            .path
            .exists()
    );
}

fn age(path: &std::path::Path, seconds: u64) {
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
    fs::File::open(path).unwrap().set_modified(then).unwrap();
}

#[tokio::test]
async fn files_the_index_doesnt_know_are_swept_at_open() {
    let dir = TempDir::new();
    let first = tier(&dir, LIMIT);
    let kept = keep(&first, KEY, Form::Still, 80, false, 100).await;
    let stray = kept.path.with_file_name("stray.webp");
    fs::write(&stray, b"left by a crash").unwrap();
    age(&stray, 120);
    fs::create_dir_all(first.temporary()).unwrap();
    let leftover = first.temporary().join("half-written");
    fs::write(&leftover, b"half").unwrap();
    age(first.temporary(), 7200);
    first.close().await;

    let second = tier(&dir, LIMIT);

    assert!(second.find(KEY, Form::Still, 80).await.is_some());
    assert!(!stray.exists());
    assert!(!leftover.exists());
}

// Another process may use the directory: its newest files and downloads aren't crash leftovers.
#[tokio::test]
async fn recent_files_survive_the_sweep() {
    let dir = TempDir::new();
    let first = tier(&dir, LIMIT);
    let kept = keep(&first, KEY, Form::Still, 80, false, 100).await;
    let moving = kept.path.with_file_name("being-indexed.webp");
    fs::write(&moving, b"renamed, not yet indexed").unwrap();
    fs::create_dir_all(first.temporary()).unwrap();
    let downloading = first.temporary().join("0.part");
    fs::write(&downloading, b"half").unwrap();

    let second = tier(&dir, LIMIT);
    second.find(KEY, Form::Still, 80).await;

    assert!(moving.exists());
    assert!(downloading.exists());
    first.close().await;
}

#[tokio::test]
async fn temporary_files_never_repeat() {
    let dir = TempDir::new();
    let tier = Arc::new(tier(&dir, LIMIT));
    let shared = tier.clone();

    let names: std::collections::HashSet<PathBuf> = (0..50)
        .map(|n| {
            if n % 2 == 0 {
                tier.temporary_file()
            } else {
                shared.temporary_file()
            }
        })
        .collect();

    assert_eq!(names.len(), 50);
    assert!(names.iter().all(|name| name.starts_with(tier.temporary())));
}

#[tokio::test]
async fn a_file_gone_from_disk_is_not_found() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);
    let kept = keep(&tier, KEY, Form::Still, 80, false, 100).await;
    fs::remove_file(&kept.path).unwrap();

    assert_eq!(tier.find(KEY, Form::Still, 80).await, None);
    assert_eq!(tier.total().await, 0);
}

#[tokio::test]
async fn a_replacement_with_another_extension_deletes_the_old_file() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);
    let old = keep(&tier, KEY, Form::Still, 80, false, 100).await;
    let temporary = download(&tier, 120).await;

    let new = tier
        .insert(NewFile {
            key: KEY.to_owned(),
            form: Form::Still,
            width: 80,
            height: 80,
            animated: false,
            full: false,
            extension: "png",
            bytes: 120,
            temporary,
        })
        .await
        .unwrap();

    assert_ne!(new.path, old.path);
    assert!(!old.path.exists());
    assert_eq!(tier.total().await, 120);
}

#[tokio::test]
async fn eviction_that_cant_delete_gives_up() {
    let dir = TempDir::new();
    let tier = tier(&dir, 1000);
    tier.set_clock(1_000);
    keep(&tier, "a", Form::Still, 80, false, 600).await;
    tier.fail_deletes();
    tier.set_clock(2_000);

    let kept = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        keep(&tier, "b", Form::Still, 80, false, 600),
    )
    .await;

    assert!(kept.is_ok(), "the image thread kept answering");
    assert!(tier.find("b", Form::Still, 80).await.is_some());
}

#[tokio::test]
async fn a_deleted_directory_stays_gone() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);
    keep(&tier, KEY, Form::Still, 80, false, 100).await;
    tier.delete().await;
    assert!(tier.is_deleted());

    // A download that was already on its way.
    fs::create_dir_all(tier.temporary()).unwrap();
    let late = tier.temporary_file();
    fs::write(&late, b"late").unwrap();
    let kept = tier
        .insert(NewFile {
            key: KEY.to_owned(),
            form: Form::Still,
            width: 80,
            height: 80,
            animated: false,
            full: false,
            extension: "webp",
            bytes: 4,
            temporary: late,
        })
        .await;

    assert_eq!(kept, None);
    assert!(!dir.path().join("1234").exists());
}

// Each process counts what it wrote; the index has what every process wrote.
#[tokio::test]
async fn the_size_counts_what_other_processes_kept() {
    let dir = TempDir::new();
    let first = tier(&dir, 10_000);
    first.find("ready", Form::Still, 0).await;
    let second = tier(&dir, 10_000);
    first.set_clock(1_000);
    second.set_clock(2_000);
    keep(&first, "big", Form::Still, 80, false, 9_000).await;

    for n in 0..70 {
        keep(&second, &format!("small/{n}"), Form::Still, 80, false, 30).await;
    }

    assert_eq!(
        second.find("big", Form::Still, 80).await,
        None,
        "evicted by the other"
    );
    assert!(second.total().await <= 9_000);
    first.close().await;
}

#[tokio::test]
async fn clearing_deletes_every_file_and_keeps_working() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);
    let kept = keep(&tier, KEY, Form::Still, 80, false, 100).await;

    tier.clear().await;

    assert!(!kept.path.exists());
    assert_eq!(tier.find(KEY, Form::Still, 80).await, None);
    assert_eq!(tier.total().await, 0);
    assert!(
        keep(&tier, KEY, Form::Still, 80, false, 100)
            .await
            .path
            .exists()
    );
}

#[tokio::test]
async fn deleting_removes_the_directory_for_good() {
    let dir = TempDir::new();
    let tier = tier(&dir, LIMIT);
    keep(&tier, KEY, Form::Still, 80, false, 100).await;

    tier.delete().await;

    assert!(!dir.path().join("1234").exists());
    assert_eq!(tier.find(KEY, Form::Still, 80).await, None);
    let temporary = dir.path().join("elsewhere");
    fs::write(&temporary, b"x").unwrap();
    let kept = tier
        .insert(NewFile {
            key: KEY.to_owned(),
            form: Form::Still,
            width: 80,
            height: 80,
            animated: false,
            full: false,
            extension: "webp",
            bytes: 1,
            temporary,
        })
        .await;
    assert_eq!(kept, None);
    assert!(!dir.path().join("1234").exists());
}

#[test]
fn file_names_carry_no_key_text() {
    let name = file_name("avatar/123/../../etc", Form::Gif, 80, "gif");

    assert!(!name.contains("avatar") && !name.contains(".."), "{name}");
    assert_eq!(name.matches('/').count(), 1);
    assert!(name.ends_with("-2-80.gif"));
}

#[tokio::test]
async fn eviction_leaves_room_below_the_limit() {
    let dir = TempDir::new();
    let tier = tier(&dir, 1000);
    for (at, key) in ["a", "b", "c", "d"].into_iter().enumerate() {
        tier.set_clock(1_000 + 100 * at as i64);
        keep(&tier, key, Form::Still, 80, false, 250).await;
    }
    tier.set_clock(2_000);

    keep(&tier, "e", Form::Still, 80, false, 250).await;

    assert_eq!(tier.total().await, 750);
    assert_eq!(tier.find("a", Form::Still, 80).await, None);
    assert_eq!(tier.find("b", Form::Still, 80).await, None);
    assert!(tier.find("c", Form::Still, 80).await.is_some());
}

#[tokio::test]
async fn two_processes_opening_a_new_directory_at_once_both_work() {
    for _ in 0..10 {
        let dir = TempDir::new();
        let (first, second) = (tier(&dir, LIMIT), tier(&dir, LIMIT));

        let (one, two) = tokio::join!(
            keep(&first, "one", Form::Still, 80, false, 10),
            keep(&second, "two", Form::Still, 80, false, 10)
        );

        assert!(one.path.exists() && two.path.exists());
        assert!(first.find("two", Form::Still, 80).await.is_some());
        assert!(second.find("one", Form::Still, 80).await.is_some());
        first.close().await;
        second.close().await;
    }
}

// Logout and forget_account must not leave a user's images because this process never
// opened the index.
#[tokio::test]
async fn deleting_works_without_an_open_index() {
    let dir = TempDir::new();
    let account = dir.path().join("1234");
    fs::create_dir_all(account.join(INDEX)).unwrap();
    fs::create_dir_all(account.join("files/ab")).unwrap();
    fs::write(account.join("files/ab/kept.webp"), b"an image").unwrap();
    let tier = tier(&dir, LIMIT);
    assert_eq!(tier.find(KEY, Form::Still, 80).await, None);

    tier.delete().await;

    assert!(!account.exists());
}

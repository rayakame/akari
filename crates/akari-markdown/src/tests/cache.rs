use std::sync::Arc;

use crate::{Cache, parse};

fn doc(source: &str) -> Arc<crate::Document> {
    Arc::new(parse(source))
}

#[test]
fn a_document_is_found_after_it_was_inserted() {
    let mut cache = Cache::new(4);
    cache.insert(1u64, 3, doc("abc"));
    assert_eq!(cache.get(&1, 3), Some(doc("abc")));
    assert_eq!(cache.get(&2, 3), None);
    assert_eq!(cache.len(), 1);
}

#[test]
fn another_fingerprint_is_a_miss() {
    let mut cache = Cache::new(4);
    cache.insert(1u64, 3, doc("abc"));
    assert_eq!(cache.get(&1, 4), None);
}

#[test]
fn the_least_recently_used_goes_first() {
    let mut cache = Cache::new(3);
    for key in 1u64..=3 {
        cache.insert(key, 1, doc("a"));
    }
    assert!(cache.get(&1, 1).is_some());
    cache.insert(4, 1, doc("a"));
    assert_eq!(cache.len(), 3);
    assert!(cache.get(&2, 1).is_none(), "2 was used least recently");
    for key in [1, 3, 4] {
        assert!(cache.get(&key, 1).is_some(), "{key}");
    }
}

#[test]
fn inserting_a_key_again_replaces_it() {
    let mut cache = Cache::new(3);
    cache.insert(1u64, 1, doc("a"));
    cache.insert(1, 1, doc("b"));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get(&1, 1), Some(doc("b")));
}

#[test]
fn the_cache_stays_within_its_capacity() {
    let mut cache = Cache::new(64);
    let shared = doc("a");
    for key in 0u64..10_000 {
        cache.insert(key, 1, shared.clone());
        if key % 3 == 0 {
            let _ = cache.get(&(key / 2), 1);
        }
        assert!(cache.len() <= 64);
    }
    assert_eq!(cache.len(), 64);
    assert!(cache.get(&9_999, 1).is_some());
}

#[test]
fn a_cache_of_capacity_zero_keeps_nothing() {
    let mut cache = Cache::new(0);
    cache.insert(1u64, 1, doc("a"));
    assert_eq!(cache.len(), 0);
}

pub(crate) fn heap_size(doc: &crate::Document) -> usize {
    use std::mem::size_of;
    let blocks: usize = doc
        .blocks()
        .iter()
        .map(|b| {
            size_of::<crate::Block>()
                + b.containers.len() * size_of::<crate::Container>()
                + b.inlines.len() * size_of::<crate::Inline>()
        })
        .sum();
    let links: usize = doc
        .links()
        .iter()
        .map(|l| {
            size_of::<crate::Link>()
                + size_of::<crate::Span>()
                + l.url.len()
                + match &l.kind {
                    crate::LinkKind::Masked { title: Some(t) } => t.len(),
                    _ => 0,
                }
        })
        .sum();
    size_of::<crate::Document>() + blocks + links
}

#[test]
#[ignore = "prints the documents' estimated heap sizes, for the cache's capacity (D57)"]
fn document_sizes() {
    use super::corpus::{cases, corpus_dir, files};
    let mut sizes: Vec<(usize, String)> = Vec::new();
    for (file, text) in files(&corpus_dir()) {
        for case in cases(&file, &text) {
            sizes.push((
                heap_size(&parse(&case.input)),
                format!("{file} / {}", case.name),
            ));
        }
    }
    sizes.sort();
    let total: usize = sizes.iter().map(|(s, _)| s).sum();
    println!("documents: {}", sizes.len());
    println!("mean: {} bytes", total / sizes.len());
    println!("median: {} bytes", sizes[sizes.len() / 2].0);
    for (size, name) in sizes.iter().rev().take(3) {
        println!("largest: {size} bytes ({name})");
    }
}

#[test]
fn an_insert_says_which_key_it_dropped() {
    let mut cache = Cache::new(2);
    assert_eq!(cache.insert(1u64, 1, doc("a")), None);
    assert_eq!(cache.insert(2, 1, doc("a")), None);
    assert_eq!(
        cache.insert(2, 1, doc("b")),
        None,
        "replacing drops nothing"
    );
    assert_eq!(cache.insert(3, 1, doc("a")), Some(1));
}

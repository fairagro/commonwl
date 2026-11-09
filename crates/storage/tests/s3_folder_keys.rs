//! These tests need a real S3 store, start one with `.dev/tes_env.sh`. They are ignored by default.
use cwl_engine_storage::{Storage, s3_storage::S3Storage};
use std::fs;
use tempfile::tempdir;
use url::Url;
use uuid::Uuid;

fn new_prefix() -> Url {
    Url::parse(&format!(
        "s3://commonwl-bucket/storage-test-{}/",
        Uuid::new_v4()
    ))
    .unwrap()
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn created_dir_is_reported_empty() {
    let storage = S3Storage::new();
    let base = new_prefix();

    storage
        .create_dir(&base.join("a/b").unwrap())
        .await
        .unwrap();

    assert_eq!(storage.list_empty_dirs(&base).await.unwrap(), vec!["a/b"]);
    assert_eq!(
        storage
            .list_empty_dirs(&base.join("a/b/").unwrap())
            .await
            .unwrap(),
        vec![""]
    );
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn empty_dir_with_a_space_keeps_its_raw_name() {
    let storage = S3Storage::new();
    let base = new_prefix();
    let dir = base.join("sp ace/").unwrap();

    storage.create_dir(&dir).await.unwrap();

    assert_eq!(
        storage.list_empty_dirs(&base).await.unwrap(),
        vec!["sp ace"]
    );
    assert_eq!(storage.list_empty_dirs(&dir).await.unwrap(), vec![""]);
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn dir_stops_being_empty_once_it_has_content() {
    let storage = S3Storage::new();
    let base = new_prefix();
    let local = tempdir().unwrap();
    let file = local.path().join("f.txt");
    fs::write(&file, "hi").unwrap();

    storage.create_dir(&base.join("a/").unwrap()).await.unwrap();
    storage
        .upload(&file, &base.join("a/f.txt").unwrap())
        .await
        .unwrap();

    assert_eq!(
        storage.list_empty_dirs(&base).await.unwrap(),
        Vec::<String>::new()
    );
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn folder_key_counts_as_directory() {
    let storage = S3Storage::new();
    let base = new_prefix();
    storage.create_dir(&base.join("a/").unwrap()).await.unwrap();

    assert!(storage.is_dir(&base.join("a/").unwrap()).await.unwrap());
    assert!(storage.is_dir(&base.join("a").unwrap()).await.unwrap());
    assert!(storage.exists(&base.join("a/").unwrap()).await.unwrap());
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn glob_returns_folder_keys_only_as_directories() {
    let storage = S3Storage::new();
    let base = new_prefix();
    for dir in ["a_dir/", "b_dir/", "skip/"] {
        storage.create_dir(&base.join(dir).unwrap()).await.unwrap();
    }

    let matches: Vec<_> = storage.glob(&base, "[a,b]_dir").await.unwrap().collect();
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(cwl_engine_storage::StoragePath::is_dir));

    let inside: Vec<_> = storage
        .glob(&base.join("a_dir/").unwrap(), "*")
        .await
        .unwrap()
        .collect();
    assert!(inside.is_empty(), "a folder key must not list itself");
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn download_creates_empty_directories() {
    let storage = S3Storage::new();
    let base = new_prefix();
    let local = tempdir().unwrap();
    let dest = local.path().join("out");

    storage
        .create_dir(&base.join("sub/deep/").unwrap())
        .await
        .unwrap();
    storage.download(&base, &dest).await.unwrap();
    assert!(dest.join("sub/deep").is_dir());

    // a directory that holds nothing but itself
    let single = local.path().join("single");
    storage
        .download(&base.join("sub/deep/").unwrap(), &single)
        .await
        .unwrap();
    assert!(single.is_dir());
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn upload_keeps_empty_subdirectories() {
    let storage = S3Storage::new();
    let base = new_prefix();
    let local = tempdir().unwrap();
    fs::create_dir_all(local.path().join("tree/empty")).unwrap();
    fs::write(local.path().join("tree/f.txt"), "hi").unwrap();

    storage.upload(local.path(), &base).await.unwrap();

    assert_eq!(
        storage.list_empty_dirs(&base).await.unwrap(),
        vec!["tree/empty"]
    );
    storage.delete(&base).await.unwrap();
}

#[tokio::test]
#[ignore = "needs a S3 store"]
async fn listings_are_not_cut_off_after_one_page() {
    const FILES: usize = 1005;
    let storage = S3Storage::new();
    let base = new_prefix();
    let local = tempdir().unwrap();
    for i in 0..FILES {
        fs::write(local.path().join(format!("f{i}")), "").unwrap();
    }

    storage.upload(local.path(), &base).await.unwrap();

    assert_eq!(storage.glob(&base, "*").await.unwrap().count(), FILES);
    let dest = local.path().join("copy");
    storage.download(&base, &dest).await.unwrap();
    assert_eq!(fs::read_dir(&dest).unwrap().count(), FILES);
    storage.delete(&base).await.unwrap();
}

use crate::{Storage, StoragePath};
use anyhow::Context;
use async_trait::async_trait;
use aws_sdk_s3 as s3;
use aws_sdk_s3::config::RequestChecksumCalculation;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use glob::Pattern;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;
use url::Url;

/// Explicit S3 connection details, for a caller that has its own config source instead of the
/// environment
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint_url: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    /// Most S3-compatible (non-AWS) stores ignore this but the SDK still requires one be set;
    /// defaults to `"us-east-1"` if not given.
    pub region: Option<String>,
}

#[derive(Debug, Clone)]
pub struct S3Storage {
    client: OnceCell<Arc<s3::Client>>,
    config: Option<S3Config>,
}

impl S3Storage {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: OnceCell::new(),
            config: None,
        }
    }

    /// Like [`S3Storage::new`], but built from `config` instead of reading
    /// `S3_ENDPOINT_URL`/the AWS SDK's env-based credential chain.
    #[must_use]
    pub fn with_config(config: S3Config) -> Self {
        Self {
            client: OnceCell::new(),
            config: Some(config),
        }
    }

    async fn client(&self) -> anyhow::Result<Arc<s3::Client>> {
        self.client
            .get_or_try_init(|| async {
                if let Some(config) = &self.config {
                    let credentials = aws_sdk_s3::config::Credentials::new(
                        config.access_key_id.clone(),
                        config.secret_access_key.clone(),
                        config.session_token.clone(),
                        None,
                        "sciwin-settings",
                    );
                    let region = config
                        .region
                        .clone()
                        .unwrap_or_else(|| "us-east-1".to_string());
                    return Ok(Arc::new(aws_sdk_s3::Client::from_conf(
                        s3::config::Builder::new()
                            .endpoint_url(&config.endpoint_url)
                            .credentials_provider(credentials)
                            .region(aws_sdk_s3::config::Region::new(region))
                            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
                            .force_path_style(true)
                            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
                            .build(),
                    )));
                }

                dotenvy::dotenv().ok();

                let endpoint_url = std::env::var("S3_ENDPOINT_URL")?;
                let config = aws_config::load_from_env().await;
                Ok(Arc::new(aws_sdk_s3::Client::from_conf(
                    s3::config::Builder::from(&config)
                        .endpoint_url(endpoint_url)
                        .force_path_style(true)
                        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
                        .build(),
                )))
            })
            .await
            .cloned()
    }

    /// Parses <s3://bucket/key> or "bucket/key" into (bucket, key)
    fn parse_uri(uri: &Url) -> anyhow::Result<(String, String)> {
        let bucket = uri
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Missing bucket"))?;
        let key = uri.path().trim_start_matches('/');
        Ok((bucket.to_string(), key.to_string()))
    }

    /// Like `parse_uri`, but for keys holding the name a TES worker sees: `a b`, not `a%20b`.
    fn parse_raw_uri(uri: &Url) -> anyhow::Result<(String, String)> {
        let (bucket, key) = Self::parse_uri(uri)?;
        Ok((bucket, urlencoding::decode(&key)?.into_owned()))
    }
}

impl Default for S3Storage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn upload(&self, local: &Path, dest: &Url) -> anyhow::Result<()> {
        let (bucket, key) = S3Storage::parse_uri(dest)?;
        if local.is_file() {
            self.upload_file(local, &bucket, &key).await?;
        } else {
            let mut set = tokio::task::JoinSet::new();
            let sem = Arc::new(tokio::sync::Semaphore::new(32));
            let mut is_empty = true;
            for item in local.read_dir()? {
                is_empty = false;
                let dir_dest = if dest.path().ends_with('/') {
                    dest.clone()
                } else {
                    Url::parse(&format!("{dest}/"))?
                };

                let item = item?;
                let path = item.path();
                if let Some(filename) = path.file_name() {
                    let dest = dir_dest.join(&filename.to_string_lossy())?;
                    let this = self.clone();

                    let permit = sem.clone().acquire_owned().await?;
                    set.spawn(async move {
                        let _permit = permit; //reference semaphore
                        this.upload(&path, &dest).await
                    });
                }
            }
            if is_empty {
                self.create_dir(dest).await?;
            }
            while let Some(res) = set.join_next().await {
                res??;
            }
        }
        Ok(())
    }

    async fn download(&self, src: &Url, local: &Path) -> anyhow::Result<()> {
        let (bucket, key) = S3Storage::parse_uri(src)?;
        let file_type = self.s3_file_type(&bucket, &key).await?;
        match file_type {
            S3PathType::File => self.download_file(&bucket, &key, local).await?,
            S3PathType::Directory => {
                let keys = self.list_keys(&bucket, &key).await?;
                let mut set = tokio::task::JoinSet::new();
                let sem = Arc::new(tokio::sync::Semaphore::new(32));
                for obj_key in keys {
                    let relative = obj_key
                        .strip_prefix(&key)
                        .unwrap_or(&obj_key)
                        .trim_start_matches('/');
                    let local_path = local.join(relative);
                    if obj_key.ends_with('/') {
                        tokio::fs::create_dir_all(&local_path)
                            .await
                            .with_context(|| {
                                format!("Could not create directory {}", local_path.display())
                            })?;
                        continue;
                    }
                    if let Some(parent) = local_path.parent() {
                        tokio::fs::create_dir_all(parent).await.with_context(|| {
                            format!("Could not create directory {}", parent.display())
                        })?;
                    }
                    let permit = sem.clone().acquire_owned().await?;

                    let bucket = bucket.clone();
                    let this = self.clone();
                    set.spawn(async move {
                        let _permit = permit; //reference semaphore
                        this.download_file(&bucket, &obj_key, &local_path).await
                    });
                }
                while let Some(res) = set.join_next().await {
                    res??;
                }
            }
            S3PathType::NotFound => anyhow::bail!("Could not find path {bucket}/{key}"),
        }

        Ok(())
    }

    async fn exists(&self, uri: &Url) -> anyhow::Result<bool> {
        let (bucket, key) = S3Storage::parse_uri(uri)?;

        let result = self
            .client()
            .await?
            .head_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(err) => {
                let service_err = err.into_service_error();
                if service_err.is_not_found() {
                    Ok(false)
                } else {
                    Err(anyhow::anyhow!(service_err))
                }
            }
        }
    }

    async fn is_dir(&self, uri: &Url) -> anyhow::Result<bool> {
        let (bucket, key) = S3Storage::parse_uri(uri)?;
        Ok(matches!(
            self.s3_file_type(&bucket, &key).await?,
            S3PathType::Directory
        ))
    }

    async fn delete(&self, uri: &Url) -> anyhow::Result<()> {
        let (bucket, key) = S3Storage::parse_uri(uri)?;

        let mut continuation_token = None;

        loop {
            let mut req = self
                .client()
                .await?
                .list_objects_v2()
                .bucket(&bucket)
                .prefix(&key);

            if let Some(token) = continuation_token {
                req = req.continuation_token(token);
            }

            let page = req.send().await?;
            let keys: Vec<_> = page
                .contents()
                .iter()
                .filter_map(|obj| obj.key())
                .map(|k| ObjectIdentifier::builder().key(k).build())
                .collect::<Result<_, _>>()?;

            if keys.is_empty() {
                break;
            }

            self.client()
                .await?
                .delete_objects()
                .bucket(&bucket)
                .delete(
                    Delete::builder()
                        .set_objects(Some(keys))
                        .quiet(true)
                        .build()?,
                )
                .send()
                .await?;

            // Check if there are more pages
            if page.is_truncated().unwrap_or(false) {
                continuation_token = page.next_continuation_token().map(str::to_string);
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn read_file(&self, uri: &Url) -> anyhow::Result<String> {
        let (bucket, key) = S3Storage::parse_uri(uri)?;
        let object = self
            .client()
            .await?
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await?;

        let bytes = object.body.collect().await?.into_bytes();
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    async fn glob(
        &self,
        base: &Url,
        pattern: &str,
    ) -> anyhow::Result<Box<dyn Iterator<Item = StoragePath> + Send>> {
        let (bucket, key_prefix) = S3Storage::parse_uri(base)?;

        // an empty pattern (bare `$(runtime.outdir)`) or "." means "the source_dir itself is the match"
        if pattern.is_empty() || pattern == "." {
            let dir_url = Url::parse(&format!(
                "s3://{bucket}/{}/",
                key_prefix.trim_end_matches('/')
            ))?;
            return Ok(Box::new(std::iter::once(StoragePath::Remote(dir_url))));
        }

        let pattern = Pattern::new(pattern)?;
        let strip_prefix = if key_prefix.is_empty() || key_prefix.ends_with('/') {
            key_prefix.clone()
        } else {
            format!("{key_prefix}/")
        };

        let keys = self.list_keys(&bucket, &key_prefix).await?;

        let mut file_urls = vec![];
        // S3 has no real directories, only object keys
        // a glob pattern like  "somedir" is meant to match a *directory*, which only exists here as a
        // common prefix shared by one or more object keys.
        let mut dir_matches: BTreeSet<String> = BTreeSet::new();

        // matches shell/Local glob semantics: `*` doesn't cross a `/` (e.g. `dir/*` must not also
        // match `dir/sub/file`)
        let match_options = glob::MatchOptions {
            require_literal_separator: true,
            ..glob::MatchOptions::new()
        };

        for key in &keys {
            let relative = key.strip_prefix(&strip_prefix).unwrap_or(key);

            // a folder key is only a directory, the segment walk below picks it up
            if !key.ends_with('/')
                && pattern.matches_with(relative, match_options)
                && let Ok(url) = Url::parse(&format!("s3://{bucket}/{key}"))
            {
                file_urls.push(StoragePath::Remote(url));
            }

            let segments: Vec<&str> = relative.split('/').collect();
            for i in 1..segments.len() {
                let candidate = segments[..i].join("/");
                if pattern.matches_with(&candidate, match_options) {
                    dir_matches.insert(candidate);
                }
            }
        }

        let dir_urls = dir_matches.into_iter().filter_map(move |dir| {
            Url::parse(&format!("s3://{bucket}/{strip_prefix}{dir}/"))
                .ok()
                .map(StoragePath::Remote)
        });

        Ok(Box::new(file_urls.into_iter().chain(dir_urls)))
    }

    async fn create_dir(&self, uri: &Url) -> anyhow::Result<()> {
        let (bucket, key) = S3Storage::parse_raw_uri(uri)?;
        let key = dir_prefix(&key);
        anyhow::ensure!(!key.is_empty(), "Can not create bucket root {uri}");

        self.client()
            .await?
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(ByteStream::from_static(b""))
            .send()
            .await?;
        Ok(())
    }

    async fn list_empty_dirs(&self, base: &Url) -> anyhow::Result<Vec<String>> {
        let (bucket, key) = S3Storage::parse_raw_uri(base)?;
        let prefix = dir_prefix(&key);
        let keys = self.list_keys(&bucket, &prefix).await?;
        Ok(empty_dirs(&keys, &prefix))
    }
}

impl S3Storage {
    /// Lists every key below `prefix`, across as many pages as the store returns.
    async fn list_keys(&self, bucket: &str, prefix: &str) -> anyhow::Result<Vec<String>> {
        let mut keys = vec![];
        let mut continuation_token = None;

        loop {
            let page = self
                .client()
                .await?
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix)
                .set_continuation_token(continuation_token)
                .send()
                .await?;

            keys.extend(
                page.contents()
                    .iter()
                    .filter_map(|obj| obj.key().map(str::to_owned)),
            );

            continuation_token = page.next_continuation_token().map(str::to_string);
            if !page.is_truncated().unwrap_or(false) || continuation_token.is_none() {
                return Ok(keys);
            }
        }
    }

    async fn download_file(&self, bucket: &str, key: &str, local: &Path) -> anyhow::Result<()> {
        let resp = self
            .client()
            .await?
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await?;

        let bytes = resp.body.collect().await?.into_bytes();

        let mut file = File::create(local)
            .await
            .with_context(|| format!("Could not create file {}", local.display()))?;
        file.write_all(&bytes).await?;
        file.flush().await?;

        Ok(())
    }

    async fn upload_file(&self, local: &Path, bucket: &str, key: &str) -> anyhow::Result<()> {
        let body = ByteStream::from_path(local).await?;

        self.client()
            .await?
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body)
            .send()
            .await?;
        Ok(())
    }

    async fn s3_file_type(&self, bucket: &str, key: &str) -> anyhow::Result<S3PathType> {
        // a folder key answers HEAD like an object
        if !key.ends_with('/')
            && self
                .client()
                .await?
                .head_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .is_ok()
        {
            return Ok(S3PathType::File);
        }

        let list = self
            .client()
            .await?
            .list_objects_v2()
            .bucket(bucket)
            .prefix(key)
            .max_keys(1)
            .send()
            .await?;

        if list.contents().is_empty() {
            Ok(S3PathType::NotFound)
        } else {
            Ok(S3PathType::Directory)
        }
    }
}

#[derive(Debug)]
enum S3PathType {
    File,
    Directory,
    NotFound,
}

/// Normalizes a key to its directory form, leaving the bucket root (`""`) untouched.
fn dir_prefix(key: &str) -> String {
    match key.trim_end_matches('/') {
        "" => String::new(),
        key => format!("{key}/"),
    }
}

/// Picks the folder keys with nothing below them, relative to `prefix`.
fn empty_dirs(keys: &[String], prefix: &str) -> Vec<String> {
    let mut sorted: Vec<&str> = keys.iter().map(String::as_str).collect();
    sorted.sort_unstable();

    // sorting puts everything below a folder key directly after it
    sorted
        .iter()
        .enumerate()
        .filter(|(i, key)| {
            key.ends_with('/')
                && !sorted
                    .get(i + 1)
                    .is_some_and(|next| next.starts_with(**key))
        })
        .filter_map(|(_, key)| key.strip_prefix(prefix))
        .map(|relative| relative.trim_end_matches('/').to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parse_raw_uri_decodes_the_key() {
        let url = Url::parse("s3://bucket/run/sp%20ace/a%23b").unwrap();
        let (bucket, key) = S3Storage::parse_raw_uri(&url).unwrap();
        assert_eq!(bucket, "bucket");
        assert_eq!(key, "run/sp ace/a#b");
    }

    #[test]
    fn dir_prefix_normalizes_trailing_slash() {
        assert_eq!(dir_prefix("run/dir"), "run/dir/");
        assert_eq!(dir_prefix("run/dir/"), "run/dir/");
        assert_eq!(dir_prefix(""), "");
    }

    #[test]
    fn empty_dirs_reports_the_directory_itself() {
        assert_eq!(empty_dirs(&keys(&["run/dir/"]), "run/dir/"), vec![""]);
    }

    #[test]
    fn empty_dirs_reports_nested_paths() {
        let found = empty_dirs(
            &keys(&["run/a.txt", "run/sub/deep/", "run/sp ace/"]),
            "run/",
        );
        assert_eq!(found, vec!["sp ace", "sub/deep"]);
    }

    #[test]
    fn empty_dirs_skips_folder_keys_with_content() {
        // a tool can write into a directory staged as empty
        let found = empty_dirs(&keys(&["run/dir/", "run/dir/file"]), "run/");
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn empty_dirs_does_not_confuse_sibling_names() {
        let found = empty_dirs(&keys(&["run/a/", "run/a.txt", "run/ab/f"]), "run/");
        assert_eq!(found, vec!["a"]);
    }

    #[test]
    fn empty_dirs_handles_a_listing_without_folder_keys() {
        assert_eq!(
            empty_dirs(&keys(&["run/a.txt"]), "run/"),
            Vec::<String>::new()
        );
        assert_eq!(empty_dirs(&[], "run/"), Vec::<String>::new());
    }
}

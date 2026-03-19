use crate::Storage;
use anyhow::Context;
use async_trait::async_trait;
use aws_sdk_s3 as s3;
use aws_sdk_s3::config::RequestChecksumCalculation;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;
use url::Url;

#[derive(Debug, Clone)]
pub struct S3Storage {
    client: OnceCell<Arc<s3::Client>>,
}

impl S3Storage {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: OnceCell::new(),
        }
    }

    async fn client(&self) -> anyhow::Result<Arc<s3::Client>> {
        self.client
            .get_or_try_init(|| async {
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
            for item in local.read_dir()? {
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
                let objects = self
                    .client()
                    .await?
                    .list_objects_v2()
                    .bucket(&bucket)
                    .prefix(&key)
                    .send()
                    .await?;

                let keys: Vec<String> = objects
                    .contents()
                    .iter()
                    .filter_map(|obj| obj.key().map(str::to_owned))
                    .collect();
                let mut set = tokio::task::JoinSet::new();
                let sem = Arc::new(tokio::sync::Semaphore::new(32));
                for obj_key in keys {
                    let relative = obj_key.strip_prefix(&key).unwrap_or(&obj_key);
                    let local_path = local.join(relative);
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
}

impl S3Storage {
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
        if self
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

use crate::Storage;
use anyhow::Context;
use async_trait::async_trait;
use aws_sdk_s3 as s3;
use aws_sdk_s3::primitives::ByteStream;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub struct S3Storage {
    client: s3::Client,
}

impl S3Storage {
    pub async fn new() -> Self {
        dotenvy::dotenv().ok();
        let endpoint_url = std::env::var("S3_ENDPOINT_URL").expect("S3_ENDPOINT_URL must be set");
        let config = aws_config::load_from_env().await;
        let client = aws_sdk_s3::Client::from_conf(
            s3::config::Builder::from(&config)
                .endpoint_url(endpoint_url)
                .force_path_style(true)
                .build(),
        );
        Self { client }
    }

    /// Parses "s3://bucket/key" or "bucket/key" into (bucket, key)
    fn parse_uri(uri: &str) -> anyhow::Result<(String, String)> {
        let path = uri.strip_prefix("s3://").unwrap_or(uri);
        match path.split_once('/') {
            Some((bucket, key)) => Ok((bucket.to_string(), key.to_string())),
            None => Ok((path.to_string(), String::new())), // bucket only, no key
        }
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn upload(&self, local: &Path, dest: &str) -> anyhow::Result<()> {
        let (bucket, key) = S3Storage::parse_uri(dest)?;
        let body = ByteStream::from_path(local).await?;

        self.client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(body)
            .send()
            .await?;

        Ok(())
    }

    async fn download(&self, src: &str, local: &Path) -> anyhow::Result<()> {
        let (bucket, key) = S3Storage::parse_uri(src)?;

        if key.ends_with("/") {
            let objects = self
                .client
                .list_objects_v2()
                .bucket(&bucket)
                .prefix(&key)
                .send()
                .await?;

            for item in objects.contents() {
                let obj_key = item.key().unwrap_or_default();
                let relative = obj_key.strip_prefix(&key).unwrap_or(obj_key);
                let local_path = local.join(relative);
                if let Some(parent) = local_path.parent() {
                    tokio::fs::create_dir_all(parent).await.with_context(|| {
                        format!("Could not create directory {}", parent.display())
                    })?;
                }
                self.download_file(&bucket, obj_key, &local_path).await?;
            }
        } else {
            self.download_file(&bucket, &key, local).await?;
        }

        Ok(())
    }

    async fn exists(&self, uri: &str) -> anyhow::Result<bool> {
        let (bucket, key) = S3Storage::parse_uri(uri)?;

        let result = self
            .client
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
}

impl S3Storage {
    async fn download_file(&self, bucket: &str, key: &str, local: &Path) -> anyhow::Result<()> {
        let resp = self
            .client
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
}

use bollard::{Docker, body_full, query_parameters::BuildImageOptions};
use futures_util::TryStreamExt;
use std::path::Path;

pub(crate) async fn build_container(
    client: &Docker,
    docker_file: impl AsRef<Path>,
    tag: &str,
) -> anyhow::Result<()> {
    let mut archive = tar::Builder::new(vec![]);

    archive.append_path_with_name(docker_file, "Dockerfile")?;
    let tarball = archive.into_inner()?;

    let options = BuildImageOptions {
        dockerfile: "Dockerfile".to_string(),
        t: Some(tag.to_string()),
        rm: true,
        ..Default::default()
    };

    let mut stream = client.build_image(options, None, Some(body_full(tarball.into())));

    while let Some(msg) = stream.try_next().await? {
        if let Some(stream) = msg.stream {
            tracing::info!("{stream}");
        }
        if let Some(error) = msg.error {
            anyhow::bail!("Docker build error: {error}");
        }
    }

    Ok(())
}

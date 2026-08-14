use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

const MAX_ATTEMPTS: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

pub async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .with_context(|| format!("创建目录失败：{}", dir.display()))?;
    }

    let mut last_err = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match download_once(client, url, dest).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("下载出错（第 {attempt}/{MAX_ATTEMPTS} 次）：{e}");
                last_err = Some(e);
                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS * attempt as u64)).await;
                }
            }
        }
    }
    bail!("下载失败（已重试 {MAX_ATTEMPTS} 次）：{url}：{last_err:?}")
}

async fn download_once(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    let res = client.get(url).send().await.with_context(|| format!("请求失败：{url}"))?;
    if !res.status().is_success() {
        bail!("下载失败 HTTP {} {url}", res.status());
    }
    let mut file = File::create(dest)
        .await
        .with_context(|| format!("创建文件失败：{}", dest.display()))?;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?)
            .await
            .with_context(|| format!("写入文件失败：{}", dest.display()))?;
    }
    file.flush().await?;
    Ok(())
}

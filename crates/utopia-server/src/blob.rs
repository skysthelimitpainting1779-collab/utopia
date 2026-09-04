//! BlobStore：原始文件字节的存取接缝。
//!
//! 内容寻址——key 就是文件内容的 sha256，接口里没有"路径"概念：本地是分桶前的
//! 平铺目录、对象存储是 object key，任何 KV 都能实现。幂等、去重、不可变
//! （内容变了指纹就变，旧版永不覆盖——"版本回放有料"的物质基础）全部由
//! "内容即地址"免费获得。
//!
//! `local` 落在 `data/files/{sha256}`；`vercel` 通过控制面领取单次、短时、单路径
//! 签名 URL，再直接读写 Private Blob。调用方始终只认 sha256，因而摄入、回放、
//! 删除与版本逻辑不因后端而分叉。

use serde::Deserialize;
use std::path::PathBuf;

#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// 幂等写入：同指纹已存在即跳过或由后端把内容冲突视为已存在。
    async fn put(&self, sha256: &str, bytes: &[u8]) -> anyhow::Result<()>;
    async fn get(&self, sha256: &str) -> anyhow::Result<Vec<u8>>;
    #[allow(dead_code)] // 接口完整性：回放/GC 路径的将来消费者
    async fn exists(&self, sha256: &str) -> anyhow::Result<bool>;
    /// 真删（#268 下半）：只在库里确认没人再引用这份指纹之后调用。幂等：不存在也算成功
    async fn delete(&self, sha256: &str) -> anyhow::Result<()>;
}

/// 本地磁盘实现：`{dir}/{sha256}` 平铺存放（与历史行为逐字节一致）。
pub struct LocalBlobStore {
    dir: PathBuf,
}

impl LocalBlobStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

#[async_trait::async_trait]
impl BlobStore for LocalBlobStore {
    async fn put(&self, sha256: &str, bytes: &[u8]) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let path = self.dir.join(sha256);
        if !path.exists() {
            tokio::fs::write(&path, bytes).await?;
        }
        Ok(())
    }

    async fn get(&self, sha256: &str) -> anyhow::Result<Vec<u8>> {
        Ok(tokio::fs::read(self.dir.join(sha256)).await?)
    }

    async fn exists(&self, sha256: &str) -> anyhow::Result<bool> {
        Ok(self.dir.join(sha256).exists())
    }

    async fn delete(&self, sha256: &str) -> anyhow::Result<()> {
        match tokio::fs::remove_file(self.dir.join(sha256)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// Vercel Private Blob 实现。
///
/// 长命 Blob 凭据只住在 TypeScript 控制面；Rust 拿到的是一条只准对
/// `files/{sha256}` 做一个动作、很快过期的 URL。这里从不记录或打印那条 URL。
pub struct VercelBlobStore {
    client: reqwest::Client,
    presign_endpoint: String,
    token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresignResponse {
    presigned_url: String,
}

impl VercelBlobStore {
    pub fn new(control_plane_url: String, token: String) -> anyhow::Result<Self> {
        let base = control_plane_url.trim().trim_end_matches('/');
        if base.is_empty() {
            anyhow::bail!("Vercel Blob control-plane URL is empty");
        }
        reqwest::Url::parse(base)
            .map_err(|e| anyhow::anyhow!("invalid Vercel Blob control-plane URL: {e}"))?;
        if token.trim().is_empty() {
            anyhow::bail!("Vercel Blob control-plane token is empty");
        }
        Ok(Self {
            client: reqwest::Client::new(),
            presign_endpoint: format!("{base}/control/blob/presign"),
            token,
        })
    }

    /// 内容地址的唯一合法形状。拒绝大写与路径字符，避免同内容多 key 与路径穿越。
    pub(crate) fn pathname(sha256: &str) -> anyhow::Result<String> {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            anyhow::bail!("blob key must be a lowercase 64-character sha256");
        }
        Ok(format!("files/{sha256}"))
    }

    async fn presign(&self, sha256: &str, operation: &str) -> anyhow::Result<String> {
        let pathname = Self::pathname(sha256)?;
        let response = self
            .client
            .post(&self.presign_endpoint)
            .bearer_auth(&self.token)
            .json(&serde_json::json!({
                "pathname": pathname,
                "operation": operation,
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Blob presign request failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Blob presign request failed with HTTP {status}");
        }
        let body: PresignResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Blob presign response was invalid: {e}"))?;
        reqwest::Url::parse(&body.presigned_url)
            .map_err(|e| anyhow::anyhow!("Blob presign response contained an invalid URL: {e}"))?;
        Ok(body.presigned_url)
    }
}

#[async_trait::async_trait]
impl BlobStore for VercelBlobStore {
    async fn put(&self, sha256: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let url = self.presign(sha256, "put").await?;
        let response = self
            .client
            .put(url)
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Blob upload failed: {e}"))?;
        let status = response.status();
        // 内容寻址使冲突天然等于「同一份已存在」。
        if status.is_success() || status == reqwest::StatusCode::CONFLICT {
            return Ok(());
        }
        anyhow::bail!("Blob upload failed with HTTP {status}")
    }

    async fn get(&self, sha256: &str) -> anyhow::Result<Vec<u8>> {
        let url = self.presign(sha256, "get").await?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Blob download failed: {e}"))?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Blob download failed with HTTP {status}");
        }
        Ok(response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Blob response body failed: {e}"))?
            .to_vec())
    }

    async fn exists(&self, sha256: &str) -> anyhow::Result<bool> {
        let url = self.presign(sha256, "head").await?;
        let response = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Blob HEAD failed: {e}"))?;
        match response.status() {
            status if status.is_success() => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            status => anyhow::bail!("Blob HEAD failed with HTTP {status}"),
        }
    }

    async fn delete(&self, sha256: &str) -> anyhow::Result<()> {
        let url = self.presign(sha256, "delete").await?;
        let response = self
            .client
            .delete(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Blob delete failed: {e}"))?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        anyhow::bail!("Blob delete failed with HTTP {status}")
    }
}

#[cfg(test)]
mod tests {
    use super::{BlobStore, VercelBlobStore};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn content_address_accepts_only_lowercase_sha256() {
        assert_eq!(
            VercelBlobStore::pathname(SHA).unwrap(),
            format!("files/{SHA}")
        );
        assert!(VercelBlobStore::pathname("../secret").is_err());
        assert!(VercelBlobStore::pathname(&SHA.to_uppercase()).is_err());
        assert!(VercelBlobStore::pathname("abc").is_err());
    }

    #[tokio::test]
    async fn put_requests_a_scoped_url_then_uploads_the_bytes() {
        let server = MockServer::start().await;
        let object_url = format!("{}/object", server.uri());

        Mock::given(method("POST"))
            .and(path("/control/blob/presign"))
            .and(header("authorization", "Bearer internal-secret"))
            .and(body_json(serde_json::json!({
                "pathname": format!("files/{SHA}"),
                "operation": "put"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "presignedUrl": object_url })),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/object"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let store = VercelBlobStore::new(server.uri(), "internal-secret".into()).unwrap();
        store.put(SHA, b"hello").await.unwrap();
    }

    #[tokio::test]
    async fn a_missing_blob_is_reported_as_not_existing() {
        let server = MockServer::start().await;
        let object_url = format!("{}/missing", server.uri());

        Mock::given(method("POST"))
            .and(path("/control/blob/presign"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "presignedUrl": object_url })),
            )
            .mount(&server)
            .await;
        Mock::given(method("HEAD"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let store = VercelBlobStore::new(server.uri(), "internal-secret".into()).unwrap();
        assert!(!store.exists(SHA).await.unwrap());
    }
}

//! BlobStore：原始文件字节的存取接缝。
//!
//! 内容寻址——key 就是文件内容的 sha256，接口里没有"路径"概念：本地是分桶前的
//! 平铺目录、对象存储是 object key，任何 KV 都能实现。幂等、去重、不可变
//! （内容变了指纹就变，旧版永不覆盖——"版本回放有料"的物质基础）全部由
//! "内容即地址"免费获得。
//!
//! 现阶段唯一实现是本地磁盘（data/files/{sha256}）。将来接对象存储/网盘
//! （P5 连接器、多实例部署共享存储）只需新增实现，摄入/上传/解析/回放的
//! 调用方一行不改。配置入口 UTOPIA_BLOB_BACKEND 预留，当前仅接受 "local"。

use std::path::PathBuf;

#[async_trait::async_trait]
pub trait BlobStore: Send + Sync {
    /// 幂等写入：同指纹已存在即跳过。
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

#[cfg(test)]
mod tests {
    use super::{BlobStore, VercelBlobStore};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn content_address_accepts_only_lowercase_sha256() {
        assert_eq!(VercelBlobStore::pathname(SHA).unwrap(), format!("files/{SHA}"));
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

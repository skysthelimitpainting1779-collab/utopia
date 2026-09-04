use figment::{
    providers::{Env, Serialized},
    Figment,
};
use serde::{Deserialize, Serialize};

/// 全局配置。来源优先级：环境变量（前缀 `UTOPIA_`）> 默认值。
/// `.env` 文件由二进制入口通过 dotenvy 预加载进环境变量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    /// 跑迁移用的连接串。迁移要建表建触发器，运行时不需要那些权限——分开之后
    /// 应用可以用一个只读写业务表、对台账只增不改的受限角色连库。
    /// 不设则回落到 `database_url`：既有部署无需改动即可照常升级。
    pub migration_url: Option<String>,
    pub bind_addr: String,
    /// Vercel 等无持久进程托管环境的总开关。缺省关闭，原来的自部署行为不变。
    pub hosted: bool,
    /// 原始文件后端：`local`（缺省）或 `vercel`。
    pub blob_backend: String,
    /// 文档词法检索后端：`tantivy`（缺省）或 `postgres`。
    pub lexical_backend: String,
    /// 托管控制面的显式地址。Vercel 上可留空并从 `VERCEL_URL` 推出。
    pub control_plane_url: Option<String>,
    /// Rust 内部端点与 Blob 签名控制面共用的部署密钥。
    pub control_plane_token: Option<String>,
    /// JWT 签名密钥。留空则首次启动时自动生成并存进 deployment_settings——
    /// 要求部署者手填一个随机串，现实中的结果是默认值原样上生产。
    /// 显式给出时优先于库里那条：密钥轮换与多实例显式对齐走这条路。
    pub jwt_secret: Option<String>,
    /// 凭据封印钥匙（32 字节，64 位十六进制或 base64）。留空则用数据目录下的
    /// `secret.key`，首次启动生成。**钥匙不进库**：库泄漏不等于凭据泄漏，是静态加密
    /// 的全部意义；备份数据目录时把它一起带走，没有它库里的凭据读不出来。
    pub secret_key: Option<String>,
    /// 前端构建产物目录；存在时由服务端托管 SPA（history fallback）。
    pub web_dist: String,
    /// 数据目录：本地模式下放原始文件与 Tantivy；托管模式下只能当临时 scratch。
    pub data_dir: String,
    /// 数据库连接池上限。缺省 32，与 worker 并发的缺省对齐——池子小于并发时
    /// 症状是请求变慢而不是任何一处说"池子不够"，所以它必须可调。
    pub db_max_connections: Option<u32>,
    /// 强制给会话 cookie 打 Secure。缺省 false：由请求的 X-Forwarded-Proto 判定，
    /// 走 TLS 才打。只有代理不发那个头时才需要在这里强制打开。
    pub cookie_secure: bool,
    /// 是否开放注册。false 时仅首个用户（引导部署）可注册，其余需管理员开放。
    pub open_registration: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "postgres://utopia:utopia@localhost:1517/utopia".into(),
            migration_url: None,
            bind_addr: "0.0.0.0:1516".into(),
            hosted: false,
            blob_backend: "local".into(),
            lexical_backend: "tantivy".into(),
            control_plane_url: None,
            control_plane_token: None,
            jwt_secret: None,
            secret_key: None,
            web_dist: "web/dist".into(),
            data_dir: "data".into(),
            db_max_connections: None,
            cookie_secure: false,
            open_registration: true,
        }
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let cfg: Self = Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Env::prefixed("UTOPIA_"))
            .extract()?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// 迁移连接串：未单独配置时用运行时那一个。
    pub fn migration_url(&self) -> &str {
        self.migration_url.as_deref().unwrap_or(&self.database_url)
    }

    /// Vercel 会给容器传 `PORT`。有它时必须覆盖静态 bind_addr，并且只允许数字端口。
    pub fn runtime_bind_addr(&self) -> anyhow::Result<String> {
        self.bind_addr_with_port(std::env::var("PORT").ok().as_deref())
    }

    fn bind_addr_with_port(&self, port: Option<&str>) -> anyhow::Result<String> {
        match port.map(str::trim).filter(|v| !v.is_empty()) {
            Some(value) => {
                let parsed: u16 = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("PORT must be an integer from 1 to 65535"))?;
                if parsed == 0 {
                    anyhow::bail!("PORT must be an integer from 1 to 65535");
                }
                Ok(format!("0.0.0.0:{parsed}"))
            }
            None => Ok(self.bind_addr.clone()),
        }
    }

    /// 托管模式不能把凭据封印钥匙落在临时盘，也不能留下未知后端静默回退。
    pub fn validate(&self) -> anyhow::Result<()> {
        if !matches!(self.blob_backend.as_str(), "local" | "vercel") {
            anyhow::bail!("UTOPIA_BLOB_BACKEND must be `local` or `vercel`");
        }
        if !matches!(self.lexical_backend.as_str(), "tantivy" | "postgres") {
            anyhow::bail!("UTOPIA_LEXICAL_BACKEND must be `tantivy` or `postgres`");
        }
        if self.hosted {
            if self
                .secret_key
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                anyhow::bail!("UTOPIA_SECRET_KEY is required when UTOPIA_HOSTED=true");
            }
            if self
                .control_plane_token
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                anyhow::bail!(
                    "UTOPIA_CONTROL_PLANE_TOKEN is required when UTOPIA_HOSTED=true"
                );
            }
        }
        if self.blob_backend == "vercel"
            && self
                .control_plane_token
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            anyhow::bail!("UTOPIA_CONTROL_PLANE_TOKEN is required for Vercel Blob");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn defaults_preserve_the_self_hosted_runtime() {
        let cfg = AppConfig::default();
        assert!(!cfg.hosted);
        assert_eq!(cfg.blob_backend, "local");
        assert_eq!(cfg.lexical_backend, "tantivy");
        assert_eq!(cfg.runtime_bind_addr().unwrap(), cfg.bind_addr);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn a_runtime_port_overrides_the_static_bind_address() {
        let cfg = AppConfig::default();
        assert_eq!(
            cfg.bind_addr_with_port(Some("8080")).unwrap(),
            "0.0.0.0:8080"
        );
        assert!(cfg.bind_addr_with_port(Some("0")).is_err());
        assert!(cfg.bind_addr_with_port(Some("not-a-port")).is_err());
    }

    #[test]
    fn hosted_mode_requires_non_ephemeral_secrets() {
        let mut cfg = AppConfig {
            hosted: true,
            ..AppConfig::default()
        };
        let missing_key = cfg.validate().unwrap_err().to_string();
        assert!(missing_key.contains("UTOPIA_SECRET_KEY"));

        cfg.secret_key = Some("00".repeat(32));
        let missing_token = cfg.validate().unwrap_err().to_string();
        assert!(missing_token.contains("UTOPIA_CONTROL_PLANE_TOKEN"));

        cfg.control_plane_token = Some("test-token".into());
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn unknown_backends_are_rejected_instead_of_falling_back() {
        let mut cfg = AppConfig::default();
        cfg.blob_backend = "mystery".into();
        assert!(cfg.validate().unwrap_err().to_string().contains("BLOB_BACKEND"));

        cfg.blob_backend = "local".into();
        cfg.lexical_backend = "mystery".into();
        assert!(cfg
            .validate()
            .unwrap_err()
            .to_string()
            .contains("LEXICAL_BACKEND"));
    }
}

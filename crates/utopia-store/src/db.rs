use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// 连接池上限的缺省。
///
/// **它不再与 worker 并发相等**（worker 缺省已是 64，见迁移 0011），这是有意的：
/// 后台任务大部分时间在等模型应答，那段时间既不占连接、也被按模型的信号量压在
/// 十来个以内。池子要覆盖的是**真在干活**的那些——每块的 epoch 检查、向量检索、
/// 未匹配统计这类短查询，它们会成串涌来。
///
/// 曾经写死 10，而 worker 默认 32——三倍超发，撞上来会先是请求变慢再是超时，
/// 而不是任何一处报错说"池子不够"。所以这个数的判据是"同时在跑的短查询有多少"，
/// 不是"有多少个任务槽位"；worker 再往上提时该重量的是前者。
const DEFAULT_MAX_CONNECTIONS: u32 = 32;

/// Vercel 容器会横向扩容并在请求结束后继续保温。连 Supavisor **session mode** 时，
/// 每个保温容器哪怕什么都不干，也会一直占着它打开过的数据库 session。项目的
/// session pool 很小（常见 15）；如果每个实例都长期保留自己的池，几个页面请求就能
/// 把整池占满，之后连静态资源请求触发的冷启动都会在数据库建连处直接 500。
///
/// 托管模式因此只允许每实例最多两个并发连接，并在每次借用结束时主动关闭连接，
/// 而不是把它放回空闲队列。后者比 idle_timeout 更重要：Vercel 可以冻结温实例，
/// 冻结期间 Tokio 的清理计时器不会运行，单靠空闲超时仍会留下数据库 session。
///
/// 这里仍保留 session mode，而不是偷偷切 transaction mode：SQLx 的高层 query API
/// 默认使用 prepared statements，而 Supavisor transaction mode 对 prepared statements
/// 有不同约束。连接模式应该由部署配置显式选择，不该在存储层猜。
const HOSTED_MAX_CONNECTIONS: u32 = 2;
const HOSTED_IDLE_SECONDS: u64 = 20;
const HOSTED_MAX_LIFETIME_SECONDS: u64 = 300;

fn hosted_runtime() -> bool {
    std::env::var("UTOPIA_HOSTED")
        .ok()
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

pub async fn connect(database_url: &str, max_connections: Option<u32>) -> anyhow::Result<PgPool> {
    let hosted = hosted_runtime();
    let max = if hosted {
        max_connections
            .unwrap_or(HOSTED_MAX_CONNECTIONS)
            .clamp(1, HOSTED_MAX_CONNECTIONS)
    } else {
        max_connections.unwrap_or(DEFAULT_MAX_CONNECTIONS).max(2)
    };

    let mut options = PgPoolOptions::new()
        .max_connections(max)
        // 取不到连接时早点响亮地失败，而不是把请求悬在默认的 30 秒上——
        // 池子配小了要看得出来是池子的问题
        .acquire_timeout(Duration::from_secs(10));

    if hosted {
        options = options
            // 不预热连接；托管实例只有真的做数据库工作时才占一个 session。
            .min_connections(0)
            // 防御性保留：在不会被冻结的运行期里，空闲连接也应尽快归还。
            .idle_timeout(Some(Duration::from_secs(HOSTED_IDLE_SECONDS)))
            .max_lifetime(Some(Duration::from_secs(HOSTED_MAX_LIFETIME_SECONDS)))
            // 关键约束：serverless/Fluid 温实例可能被冻结，冻结时 idle reaper 不跑。
            // 每次数据库借用结束就关掉 session，避免一个已经没有工作可做的实例
            // 继续占着 Supavisor session pool。需要下一条查询时再按需重连。
            .after_release(|_conn, _meta| Box::pin(async move { Ok(false) }));
    }

    let pool = options.connect(database_url).await?;
    tracing::info!(max_connections = max, hosted, "数据库连接池已建立");
    Ok(pool)
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

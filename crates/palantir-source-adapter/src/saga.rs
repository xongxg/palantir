/// Saga 编排层 — Phase 3 执行引擎时填充真实实现
/// Phase 2 只保留 trait 定义 + Noop stub，供 ActionType 应用级声明引用

use async_trait::async_trait;

// ── 数据结构 ──────────────────────────────────────────────────────────────────

/// Saga 中的单个步骤
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SagaStep {
    /// 步骤唯一标识
    pub id: String,
    /// 该步骤执行的 ActionType id
    pub action_type_id: String,
    /// 失败时的补偿 ActionType id（可选）
    pub compensate_action_type_id: Option<String>,
    /// 幂等键模板，如 "order-{customer_id}-{timestamp}"
    pub idempotency_key_template: String,
    /// 步骤描述
    pub description: Option<String>,
}

/// Saga 定义（跨 Fold / 跨 Project 的应用级 Use Case）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SagaDefinition {
    pub id: String,
    pub name: String,
    /// 有序步骤列表，按顺序执行，失败时逆序补偿
    pub steps: Vec<SagaStep>,
}

/// Saga 单次执行结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct SagaResult {
    pub saga_id: String,
    pub status: SagaStatus,
    pub completed_steps: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SagaStatus {
    Completed,
    Failed,
    Compensated,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait SagaOrchestrator: Send + Sync {
    /// 执行一个 Saga（跨 BC/Fold 的编排入口）
    async fn execute(
        &self,
        saga_def: &SagaDefinition,
        context: serde_json::Value,
    ) -> Result<SagaResult, String>;

    /// 手动触发补偿（回滚已完成的步骤）
    async fn compensate(&self, saga_id: &str) -> Result<(), String>;
}

// ── Noop Stub（Phase 2）───────────────────────────────────────────────────────

/// Phase 2 stub：声明存在，执行时返回 NotImplemented 错误
/// Phase 3 执行引擎实装后替换为真实实现
pub struct NoopSagaOrchestrator;

#[async_trait]
impl SagaOrchestrator for NoopSagaOrchestrator {
    async fn execute(
        &self,
        saga_def: &SagaDefinition,
        _context: serde_json::Value,
    ) -> Result<SagaResult, String> {
        Err(format!(
            "Saga '{}' execution not yet implemented (Phase 3). \
             Declare the saga definition now; execution engine will be wired in Phase 3.",
            saga_def.name
        ))
    }

    async fn compensate(&self, _saga_id: &str) -> Result<(), String> {
        // 补偿在 Phase 2 无副作用，安全返回 Ok
        Ok(())
    }
}

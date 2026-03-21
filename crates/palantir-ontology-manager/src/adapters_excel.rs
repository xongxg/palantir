/// Excel / ODS 文件适配器（存根）
///
/// 支持 .xlsx / .xls / .ods 格式，可指定 Sheet 名称或索引。
///
/// 依赖（待启用）：`calamine` crate（纯 Rust，无需安装 Office）
///
/// 配置示例（deployment.toml）：
/// ```toml
/// [sources.budget_excel]
/// type       = "excel"
/// path       = "data/uploads/budget_2026.xlsx"
/// sheet      = "Sheet1"       # 留空取第一个 sheet
/// header_row = 0              # 第几行是表头（0-indexed）
/// skip_rows  = 0              # 表头之前跳过几行
/// ```
use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;
use std::path::PathBuf;

pub struct ExcelAdapter {
    pub id:         String,
    pub path:       PathBuf,
    pub ns:         String,
    pub schema:     String,
    /// Sheet 名称；None = 取第一个 sheet
    pub sheet:      Option<String>,
    /// 表头所在行（0-indexed，默认 0）
    pub header_row: usize,
    /// 表头之前跳过的行数
    pub skip_rows:  usize,
}

impl ExcelAdapter {
    pub fn new(
        id: impl Into<String>,
        path: impl Into<PathBuf>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), path: path.into(),
            ns: ns.into(), schema: schema.into(),
            sheet: None, header_row: 0, skip_rows: 0,
        }
    }

    pub fn with_sheet(mut self, name: impl Into<String>) -> Self { self.sheet = Some(name.into()); self }
    pub fn with_header_row(mut self, row: usize) -> Self { self.header_row = row; self }
    pub fn with_skip_rows(mut self, n: usize) -> Self { self.skip_rows = n; self }
}

#[async_trait]
impl SourceAdapter for ExcelAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "excel" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "excel".to_string(),
            has_cursor: false,
            partitions: None,
        }
    }

    // TODO: 启用 calamine crate 后实现以下方法
    async fn test_connection(&self) -> Result<String, AdapterError> {
        Err(AdapterError::Message("Excel adapter: not yet implemented".to_string()))
    }

    async fn fetch_preview(&self, _limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        Err(AdapterError::Message("Excel adapter: not yet implemented".to_string()))
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        Err(AdapterError::Message("Excel adapter: not yet implemented".to_string()))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        Box::new(stream::iter(vec![Err(AdapterError::Message(
            "Excel adapter: not yet implemented".to_string(),
        ))]))
    }
}

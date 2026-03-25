/// S3 / 兼容对象存储适配器（S3 / AWS S3 / 阿里云 OSS / 腾讯云 COS / 华为 OBS / MinIO）
///
/// 依赖：`object_store` crate（Apache Arrow 生态，一套 API 覆盖所有 S3 兼容存储）
///
/// 支持文件格式：CSV / JSON / JSONL / Parquet（Parquet 需 arrow2 feature）
///
/// 配置示例（deployment.toml）：
/// ```toml
/// [sources.employees_s3]
/// type       = "s3"
/// bucket     = "my-data-bucket"
/// prefix     = "hr/employees/"          # 可选，只扫描此前缀下的文件
/// file_format = "csv"                   # csv | json | jsonl | parquet
/// endpoint   = "https://oss-cn-hangzhou.aliyuncs.com"  # 留空 = AWS S3
/// region     = "cn-hangzhou"
/// access_key = "${OSS_ACCESS_KEY}"
/// secret_key = "${OSS_SECRET_KEY}"
/// records_path = ""                     # JSON 嵌套时指定，如 "data"
/// ```
use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;

#[derive(Debug, Clone)]
pub enum S3FileFormat {
    Csv,
    Json,
    Jsonl,
    Parquet,
    Excel,
}

impl S3FileFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json"    => Self::Json,
            "jsonl"   => Self::Jsonl,
            "parquet" => Self::Parquet,
            "excel" | "xlsx" => Self::Excel,
            _         => Self::Csv,
        }
    }
}

pub struct S3Adapter {
    pub id:           String,
    pub ns:           String,
    pub schema:       String,

    /// S3 bucket 名称
    pub bucket:       String,
    /// 可选：只处理此前缀下的对象
    pub prefix:       Option<String>,
    /// 文件格式
    pub file_format:  S3FileFormat,
    /// 自定义 endpoint（OSS / COS / MinIO）；None = AWS S3
    pub endpoint:     Option<String>,
    pub region:       String,
    pub access_key:   String,
    pub secret_key:   String,
    /// JSON 嵌套数组路径，如 "data"
    pub records_path: Option<String>,
}

impl S3Adapter {
    pub fn new(
        id: impl Into<String>,
        bucket: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), ns: ns.into(), schema: schema.into(),
            bucket: bucket.into(), prefix: None,
            file_format: S3FileFormat::Csv,
            endpoint: None, region: "us-east-1".into(),
            access_key: String::new(), secret_key: String::new(),
            records_path: None,
        }
    }

    pub fn with_prefix(mut self, p: impl Into<String>) -> Self { self.prefix = Some(p.into()); self }
    pub fn with_format(mut self, f: S3FileFormat) -> Self { self.file_format = f; self }
    pub fn with_endpoint(mut self, e: impl Into<String>) -> Self { self.endpoint = Some(e.into()); self }
    pub fn with_region(mut self, r: impl Into<String>) -> Self { self.region = r.into(); self }
    pub fn with_credentials(mut self, ak: impl Into<String>, sk: impl Into<String>) -> Self {
        self.access_key = ak.into(); self.secret_key = sk.into(); self
    }
    pub fn with_records_path(mut self, p: impl Into<String>) -> Self {
        self.records_path = Some(p.into()); self
    }
}

#[async_trait]
impl SourceAdapter for S3Adapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "s3" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "s3".to_string(),
            has_cursor: false,
            partitions: None,
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        // TODO: 使用 object_store::aws::AmazonS3Builder 或兼容 endpoint 建立连接
        // 并执行 list(prefix) 验证权限
        Err(AdapterError::Message("S3 adapter: not yet implemented".to_string()))
    }

    async fn fetch_preview(&self, _limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        // TODO:
        // 1. 用 object_store 列出 bucket/prefix 下的文件
        // 2. 取第一个文件，按 file_format 解析
        // 3. 返回前 _limit 条记录
        Err(AdapterError::Message("S3 adapter: not yet implemented".to_string()))
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        // TODO: 取 preview(5) 后调用 discover_from_records
        Err(AdapterError::Message("S3 adapter: not yet implemented".to_string()))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        // TODO: 遍历所有匹配文件，逐行 yield CanonicalRecord
        Box::new(stream::iter(vec![Err(AdapterError::Message(
            "S3 adapter: not yet implemented".to_string(),
        ))]))
    }
}

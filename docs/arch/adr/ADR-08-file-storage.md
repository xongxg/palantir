# ADR-08: 文件存储 — RustFS

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

用户上传的原始文件存在哪里？

## 决策

**RustFS**（S3-compatible，Rust 实现单二进制），`object_store` crate 统一抽象。

## 理由

- 用户上传场景本地 FS 无法多实例共享
- RustFS 单二进制，本地开发直接启动，无 Docker 依赖
- S3-compatible，生产直接换 S3 / 云 OSS，零代码改动

## 演进路径

```
开发：LocalFileSystem（object_store crate）
生产：RustFS（S3-compatible）
云上：AWS S3 / 阿里 OSS（零代码改动）
```

## 文件元数据

存 SurrealDB（`file_upload` 对象），与 Ontology 对象关联。

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |

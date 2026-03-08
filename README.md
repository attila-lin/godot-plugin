# godot-plugin

这个仓库现在不仅提供 Godot 侧桥接，也承载 game04 的 Feishu/Bitable 通用 Rust CLI。

## Feishu CLI

以下命令默认从环境变量读取 `LARK_APP_ID` 和 `LARK_APP_SECRET`。

### Schema Sync

```bash
cargo run --example sync_bitable_schema --features ddt -- /path/to/feishu-bitable-schema-spec.json
```

game04 当前权威 schema 文件：

```bash
cargo run --example sync_bitable_schema --features ddt -- /Users/attila/workspace/game04/docs/feishu/feishu-bitable-schema-spec.json
```

### Record Upsert

```bash
cargo run --example apply_bitable_records --features ddt -- /path/to/content_record_batch.json
```

game04 当前 record batch：

```bash
cargo run --example apply_bitable_records --features ddt -- /Users/attila/workspace/game04/docs/feishu/content_record_batch.json
```

### Integrity Audit

用于检查线上表是否还有：

- 同名重复字段
- `__legacy_dup_` 遗留字段
- 同业务 key 的重复记录
- 缺 key 的记录

```bash
cargo run --example audit_bitable_integrity --features ddt -- /path/to/content_record_batch.json
```

game04 当前审计命令：

```bash
cargo run --example audit_bitable_integrity --features ddt -- /Users/attila/workspace/game04/docs/feishu/content_record_batch.json
```

返回 `{"status":"ok"}` 表示当前 batch 对应的线上表结构和记录状态是干净的；返回非零退出码表示需要清理或修复。

### Purge Legacy Fields

用于删除历史 rename 清理后遗留的 `__legacy_dup_...` 字段。

dry-run：

```bash
cargo run --example purge_legacy_bitable_fields --features ddt -- /path/to/feishu-bitable-schema-spec.json
```

实际删除：

```bash
cargo run --example purge_legacy_bitable_fields --features ddt -- /path/to/feishu-bitable-schema-spec.json --apply
```

只处理某张表：

```bash
cargo run --example purge_legacy_bitable_fields --features ddt -- /path/to/feishu-bitable-schema-spec.json --table event_definitions --apply
```

### Dedupe Records

用于按 record batch 的 `key_field` 去重，默认保留每个业务 key 的最后一条记录。

dry-run：

```bash
cargo run --example dedupe_bitable_records --features ddt -- /path/to/content_record_batch.json
```

实际删除重复记录：

```bash
cargo run --example dedupe_bitable_records --features ddt -- /path/to/content_record_batch.json --apply
```

只处理某张表：

```bash
cargo run --example dedupe_bitable_records --features ddt -- /path/to/content_record_batch.json --table countries --apply
```

## 推荐工作流

针对 game04，推荐固定顺序：

1. 更新 `game04/docs/feishu/feishu-bitable-schema-spec.json`
2. 运行 `sync_bitable_schema`
3. 生成或更新 `content_record_batch.json`
4. 运行 `apply_bitable_records`
5. 运行 `audit_bitable_integrity`
6. 如有需要，再运行 `purge_legacy_bitable_fields` / `dedupe_bitable_records`

如果要复用到其他项目，优先保持同样的数据入口：schema spec + record batch，而不是把表结构和清理逻辑散落到 shell 脚本里。
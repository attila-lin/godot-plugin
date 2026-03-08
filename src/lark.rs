//! 飞书的多维表格支持
//!
//! 直接获取数据。无需导表

mod error;
pub use error::Error;

mod bitable;
pub use bitable::Table;

mod schema;
pub use schema::{
    BitableDuplicateNameCount, BitableFieldReference, BitableFieldSpec, BitableFieldType,
    BitableIntegrityReport, BitableLegacyFieldPurgeSummary, BitableLegacyFieldPurgeTableSummary,
    BitableRecordBatch, BitableRecordBatchTable, BitableRecordDedupSummary,
    BitableRecordDedupTableSummary, BitableSchemaSpec, BitableSyncSummary,
    BitableTableIntegrityReport, BitableTableSpec, BitableUpsertSummary,
};

mod sync;

mod spreadsheet;
pub use spreadsheet::SpreadSheet;

use std::any::Any;
use std::any::type_name;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

use moka::future::Cache;
use open_lark::client::LarkClient;

/// 策划数据管理器
pub struct DataManager {
    client: DDTClient,
    /// 用于缓存数据
    cache: Cache<String, Arc<dyn Any + Send + Sync>>,
}

impl DataManager {
    pub fn singleton() -> &'static Self {
        static DATA_MANAGER: OnceLock<DataManager> = OnceLock::new();
        DATA_MANAGER.get_or_init(Self::new)
    }

    fn new() -> Self {
        let client = DDTClient::new_with_env()
            .unwrap_or_else(|err| panic!("failed to initialize lark client: {err}"));
        Self {
            client,
            cache: Cache::new(64),
        }
    }

    async fn get_cached<T>(&self, key: &'static str) -> Result<Option<T>, Error>
    where
        T: Clone + Send + Sync + 'static,
    {
        let Some(cached) = self.cache.get(key).await else {
            return Ok(None);
        };

        cached
            .downcast_ref::<T>()
            .cloned()
            .map(Some)
            .ok_or_else(|| Error::CacheTypeMismatch {
                key: key.to_owned(),
                expected: type_name::<T>(),
            })
    }

    async fn insert_cached<T>(&self, key: &'static str, value: T)
    where
        T: Send + Sync + 'static,
    {
        self.cache.insert(key.to_owned(), Arc::new(value)).await;
    }

    /// godot 中加载表格
    #[cfg(feature = "godot")]
    pub fn load_table_in_godot<T: Table>() -> Result<T::Output, Error> {
        godot_runtime().block_on(async { DataManager::singleton().load_table::<T>().await })
    }

    /// 拿到表的所有数据
    pub async fn load_table<T: Table>(&self) -> Result<T::Output, Error> {
        let name = T::table_name();
        if let Some(cached) = self.get_cached::<T::Output>(name).await? {
            return Ok(cached);
        }

        let output = T::load(&self.client).await?;
        self.insert_cached(name, output.clone()).await;
        Ok(output)
    }

    /// godot 中加载表格
    #[cfg(feature = "godot")]
    pub fn load_sheet_in_godot<S: SpreadSheet>() -> Result<Arc<S::Output>, Error> {
        godot_runtime().block_on(async { Self::singleton().load_sheet::<S>().await })
    }

    /// 拿到 spreadsheet 的所有数据
    pub async fn load_sheet<S: SpreadSheet>(&self) -> Result<Arc<S::Output>, Error> {
        let name = S::table_name();
        if let Some(cached) = self.get_cached::<Arc<S::Output>>(name).await? {
            return Ok(cached);
        }

        let arc_output = Arc::new(S::load(&self.client).await?);
        self.insert_cached(name, arc_output.clone()).await;
        Ok(arc_output)
    }

    // fn write_to_file(&self) {
    //     let mut file = std::fs::File::create(Self::output_name()).unwrap();
    //     let json = serde_json::to_string_pretty(self).unwrap();
    //     file.write_all(json.as_bytes()).unwrap();
    // }
}

pub struct DDTClient {
    lark: LarkClient,
    /// 我们默认使用一个固定的表格
    spread_sheet_token: Option<String>,
}

impl DDTClient {
    fn new(app_id: &str, app_secret: &str, spread_sheet_token: Option<String>) -> Self {
        let client = LarkClient::builder(app_id, app_secret).build();
        Self {
            lark: client,
            spread_sheet_token,
        }
    }

    /// 从环境中读取 app_id 和 app_secret
    pub fn new_with_env() -> Result<Self, Error> {
        let app_id = required_env_var("LARK_APP_ID")?;
        let app_secret = required_env_var("LARK_APP_SECRET")?;

        let spread_sheet_token = std::env::var("LARK_SPREAD_SHEET_TOKEN");
        Ok(Self::new(&app_id, &app_secret, spread_sheet_token.ok()))
    }

    pub async fn sync_bitable_schema(
        &self,
        spec: &BitableSchemaSpec,
    ) -> Result<BitableSyncSummary, Error> {
        sync::sync_bitable_schema(self, spec).await
    }

    pub async fn sync_bitable_schema_from_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<BitableSyncSummary, Error> {
        let spec = BitableSchemaSpec::from_path(path)?;
        self.sync_bitable_schema(&spec).await
    }

    pub async fn apply_record_batch(
        &self,
        batch: &BitableRecordBatch,
    ) -> Result<BitableUpsertSummary, Error> {
        sync::apply_record_batch(self, batch).await
    }

    pub async fn apply_record_batch_from_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<BitableUpsertSummary, Error> {
        let batch = BitableRecordBatch::from_path(path)?;
        self.apply_record_batch(&batch).await
    }

    pub async fn export_record_batch_from_spec(
        &self,
        spec: &BitableSchemaSpec,
    ) -> Result<BitableRecordBatch, Error> {
        sync::export_record_batch_from_spec(self, spec).await
    }

    pub async fn export_record_batch_from_spec_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<BitableRecordBatch, Error> {
        let spec = BitableSchemaSpec::from_path(path)?;
        self.export_record_batch_from_spec(&spec).await
    }

    pub async fn audit_record_batch_integrity(
        &self,
        batch: &BitableRecordBatch,
    ) -> Result<BitableIntegrityReport, Error> {
        sync::audit_record_batch_integrity(self, batch).await
    }

    pub async fn audit_record_batch_integrity_from_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<BitableIntegrityReport, Error> {
        let batch = BitableRecordBatch::from_path(path)?;
        self.audit_record_batch_integrity(&batch).await
    }

    pub async fn purge_legacy_fields_from_spec(
        &self,
        spec: &BitableSchemaSpec,
        apply: bool,
        table_filter: &[String],
    ) -> Result<BitableLegacyFieldPurgeSummary, Error> {
        sync::purge_legacy_fields_from_spec(self, spec, apply, table_filter).await
    }

    pub async fn purge_legacy_fields_from_spec_path<P: AsRef<Path>>(
        &self,
        path: P,
        apply: bool,
        table_filter: &[String],
    ) -> Result<BitableLegacyFieldPurgeSummary, Error> {
        let spec = BitableSchemaSpec::from_path(path)?;
        self.purge_legacy_fields_from_spec(&spec, apply, table_filter)
            .await
    }

    pub async fn dedupe_record_batch(
        &self,
        batch: &BitableRecordBatch,
        apply: bool,
        table_filter: &[String],
    ) -> Result<BitableRecordDedupSummary, Error> {
        sync::dedupe_record_batch(self, batch, apply, table_filter).await
    }

    pub async fn dedupe_record_batch_from_path<P: AsRef<Path>>(
        &self,
        path: P,
        apply: bool,
        table_filter: &[String],
    ) -> Result<BitableRecordDedupSummary, Error> {
        let batch = BitableRecordBatch::from_path(path)?;
        self.dedupe_record_batch(&batch, apply, table_filter).await
    }
}

fn required_env_var(name: &'static str) -> Result<String, Error> {
    std::env::var(name).map_err(|err| Error::MissingEnvVar {
        name,
        reason: err.to_string(),
    })
}

#[cfg(feature = "godot")]
fn godot_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to initialize tokio runtime for godot integration")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager_with_client() -> DataManager {
        DataManager {
            client: DDTClient::new("test-app-id", "test-app-secret", None),
            cache: Cache::new(4),
        }
    }

    #[test]
    fn cache_round_trips_arc_values() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let manager = manager_with_client();
            let value = Arc::new(String::from("sheet"));
            manager.insert_cached("sheet", value.clone()).await;

            let cached = manager
                .get_cached::<Arc<String>>("sheet")
                .await
                .expect("cached type")
                .expect("cached value");

            assert_eq!(cached.as_str(), "sheet");
        });
    }

    #[test]
    fn cache_type_mismatch_returns_error() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let manager = manager_with_client();
            manager.insert_cached("sheet", String::from("value")).await;

            let err = manager
                .get_cached::<u64>("sheet")
                .await
                .expect_err("type mismatch should be reported");

            match err {
                Error::CacheTypeMismatch { key, expected } => {
                    assert_eq!(key, "sheet");
                    assert_eq!(expected, type_name::<u64>());
                }
                other => panic!("unexpected error: {other}"),
            }
        });
    }
}

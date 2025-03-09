//! 飞书的多维表格支持
//!
//! 直接获取数据。无需导表

mod error;
pub use error::Error;

mod bitable;
pub use bitable::Table;

mod spreadsheet;
pub use spreadsheet::SpreadSheet;

use std::any::Any;
use std::sync::Arc;
use std::sync::OnceLock;

use moka::future::Cache;
use open_lark::client::LarkClient;

pub struct DateManager {
    client: DDTClient,
    /// 用于缓存数据
    cache: Cache<String, Arc<dyn Any + Send + Sync>>,
}

impl DateManager {
    pub fn singleton() -> &'static Self {
        static DATE_MANAGER: OnceLock<DateManager> = OnceLock::new();
        DATE_MANAGER.get_or_init(DateManager::new)
    }

    fn new() -> Self {
        let client = DDTClient::new_with_env();
        Self {
            client,
            cache: Cache::new(64),
        }
    }

    /// godot 中加载表格
    ///
    /// 需要用到 godot_tokio
    #[cfg(feature = "godot")]
    pub fn load_table_in_godot<T: Table>() -> Result<T::Output, Error> {
        let rt = godot_tokio::AsyncRuntime::runtime();
        tokio::task::block_in_place(move || {
            rt.block_on(async { DateManager::singleton().load_table::<T>().await })
        })
    }

    /// 拿到表的所有数据
    pub async fn load_table<T: Table>(&self) -> Result<T::Output, Error> {
        let name = T::table_name();
        if let Some(cached) = self.cache.get(name).await {
            return Ok(cached
                .downcast_ref::<T::Output>()
                .expect("Type mismatch in cache")
                .clone());
        }

        let output = T::load(&self.client).await?;
        self.cache
            .insert(name.to_owned(), Arc::new(output.clone()))
            .await;
        Ok(output)
    }

    /// godot 中加载表格
    ///
    /// 需要用到 godot_tokio
    #[cfg(feature = "godot")]
    pub fn load_sheet_in_godot<S: SpreadSheet>() -> Result<S::Output, Error> {
        let rt = godot_tokio::AsyncRuntime::runtime();
        tokio::task::block_in_place(move || {
            rt.block_on(async { DateManager::singleton().load_sheet::<S>().await })
        })
    }

    /// 拿到 spreadsheet 的所有数据
    ///
    /// FIXME: no clone
    pub async fn load_sheet<S: SpreadSheet>(&self) -> Result<S::Output, Error> {
        let name = S::table_name();
        if let Some(cached) = self.cache.get(name).await {
            return Ok(cached
                .downcast_ref::<S::Output>()
                .unwrap_or_else(|| panic!("Type mismatch in cache: {}", name))
                .clone());
        }

        let output = S::load(&self.client).await?;
        self.cache
            .insert(name.to_owned(), Arc::new(output.clone()))
            .await;
        Ok(output)
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
    pub fn new_with_env() -> Self {
        let app_id = std::env::var("LARK_APP_ID").unwrap();
        let app_secret = std::env::var("LARK_APP_SECRET").unwrap();

        let spread_sheet_token = std::env::var("LARK_SPREAD_SHEET_TOKEN");
        Self::new(&app_id, &app_secret, spread_sheet_token.ok())
    }
}

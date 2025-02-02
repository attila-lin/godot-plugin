//! 飞书的多维表格支持
//!
//! 直接获取数据。无需导表

mod error;
pub use error::Error;

use std::any::Any;
use std::sync::Arc;

use moka::future::Cache;
use open_lark::client::LarkClient;
use open_lark::service::bitable::v1::{Record, SearchAppTableRecordRequest};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::OnceCell;

pub struct DateManager {
    client: DDTClient,
    /// 用于缓存数据
    cache: Cache<String, Arc<dyn Any + Send + Sync>>,
}

impl DateManager {
    pub fn singleton() -> &'static Self {
        static DATE_MANAGER: std::sync::OnceLock<DateManager> = std::sync::OnceLock::new();
        DATE_MANAGER.get_or_init(|| DateManager::new())
    }

    fn new() -> Self {
        let client = DDTClient::new_with_env();
        Self {
            client,
            cache: Cache::new(64),
        }
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
}

struct DDTClient {
    lark: LarkClient,
}

impl DDTClient {
    fn new(app_id: &str, app_secret: &str) -> Self {
        let client = LarkClient::builder(app_id, app_secret).build();
        Self { lark: client }
    }

    /// 从环境中读取 app_id 和 app_secret
    pub fn new_with_env() -> Self {
        let app_id = std::env::var("LARK_APP_ID").unwrap();
        let app_secret = std::env::var("LARK_APP_SECRET").unwrap();
        Self::new(&app_id, &app_secret)
    }
}

pub struct Rows<T>(Vec<T>);

/// 每个数据表都可以用这个 trait 来表示
///
/// 你只要定义
///
/// + app_token: 链接上有
/// + table_id: 链接上有
pub trait Table: Sized + Clone {
    type Output: Clone + Send + Sync + 'static;

    /// 数据从多维表格中加载
    async fn load(client: &DDTClient) -> Result<Self::Output, Error> {
        let records = Self::request_records(client).await?;
        let res = Self::from_records(records);
        Ok(res)
    }

    /// 从 record 中解析出数据
    ///
    /// 需要自己实现
    fn from_records(records: Vec<Record>) -> Self::Output;

    async fn request_records(client: &DDTClient) -> Result<Vec<Record>, Error> {
        let app_token = Self::app_token();
        let table_id = Self::table_id();
        let req = SearchAppTableRecordRequest::builder()
            .app_token(app_token)
            .table_id(table_id)
            .build();
        let resp = client
            .lark
            .bitable
            .v1
            .app_table_record
            .search(req, None)
            .await
            .unwrap();
        let data = resp.data.unwrap();

        Ok(data.items)
    }

    /// 定义下 app_token
    fn app_token() -> &'static str;
    /// 定义下 table_id
    fn table_id() -> &'static str;
    /// table name
    ///
    /// 也用于导出数据的文件名
    fn table_name() -> &'static str;
}

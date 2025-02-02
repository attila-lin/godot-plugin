use open_lark::service::bitable::v1::{Record, SearchAppTableRecordRequest};
// use serde::Serialize;
// use serde_json::Value;
// use tokio::sync::OnceCell;

use super::DDTClient;
use super::Error;

/// 每个数据表都可以用这个 trait 来表示
///
/// 你只要定义
///
/// + app_token: 链接上有
/// + table_id: 链接上有
/// + table_name: 用于导出数据的文件名
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

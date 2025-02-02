use open_lark::service::sheets::v2::data_operation::ReadingSingleRangeRequest;
use serde::Serialize;
use serde_json::Value;

use super::DDTClient;
use super::Error;

/// 从表格中加载数据
pub trait SpreadSheet: Sized + Clone + Serialize {
    type Output: Clone + Send + Sync + 'static;

    fn sheet_id() -> &'static str;

    fn table_name() -> &'static str;

    fn range() -> &'static str;

    /// 数据从多维表格中加载
    async fn load(client: &DDTClient) -> Result<Self::Output, Error> {
        let data = Self::get_data(client).await?;
        let rows = data.as_array().unwrap();
        let res = Self::from_rows(rows);
        Ok(res)
    }

    /// 需要自己实现
    fn from_rows(rows: &[Value]) -> Self::Output;

    /// 从表格中加载数据
    async fn get_data(client: &DDTClient) -> Result<Value, Error> {
        assert!(
            client.spread_sheet_token.is_some(),
            "请配置环境变量 LARK_SPREAD_SHEET_TOKEN"
        );

        let range = format!("{}!{}", Self::sheet_id(), Self::range());
        let req = ReadingSingleRangeRequest::builder()
            .spreadsheet_token(client.spread_sheet_token.as_ref().unwrap())
            .range(range)
            .build();

        let resp = client
            .lark
            .sheets
            .v2
            .spreadsheet
            .reading_a_single_range(req, None)
            .await?;

        let data = resp.data.unwrap().value_range;
        Ok(data.values)
    }
}

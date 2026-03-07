use open_lark::service::sheets::v2::data_operation::ReadingSingleRangeRequest;
use serde_json::Value;

use super::DDTClient;
use super::Error;

/// 从表格中加载数据
pub trait SpreadSheet: Sized + Clone {
    type Output: Clone + Send + Sync + 'static;

    fn sheet_id() -> &'static str;

    fn table_name() -> &'static str;

    fn range() -> &'static str;

    /// 数据从多维表格中加载
    async fn load(client: &DDTClient) -> Result<Self::Output, Error> {
        let data = Self::get_data(client).await?;
        let rows = data.as_array().ok_or(Error::InvalidResponseShape {
            context: "spreadsheet values",
            expected: "array of rows",
        })?;
        let res = Self::from_rows(rows);
        Ok(res)
    }

    /// 需要自己实现
    fn from_rows(rows: &[Value]) -> Self::Output;

    /// 从表格中加载数据
    async fn get_data(client: &DDTClient) -> Result<Value, Error> {
        let spreadsheet_token =
            client
                .spread_sheet_token
                .as_deref()
                .ok_or_else(|| Error::MissingEnvVar {
                    name: "LARK_SPREAD_SHEET_TOKEN",
                    reason: "spreadsheet loading requires this token".to_owned(),
                })?;

        let range = format!("{}!{}", Self::sheet_id(), Self::range());
        let req = ReadingSingleRangeRequest::builder()
            .spreadsheet_token(spreadsheet_token)
            .range(range)
            .build();

        let resp = client
            .lark
            .sheets
            .v2
            .spreadsheet
            .reading_a_single_range(req, None)
            .await?;

        let data = resp.data.ok_or(Error::MissingResponseData {
            context: "spreadsheet range response",
        })?;
        Ok(data.value_range.values)
    }
}

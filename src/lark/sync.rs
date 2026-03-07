use std::collections::HashMap;

use open_lark::service::bitable::v1::Record;
use open_lark::service::bitable::v1::app_table::{
    CreateTableRequest, ListTablesRequest, TableData,
};
use open_lark::service::bitable::v1::app_table_record::{
    CreateRecordRequest, SearchRecordRequest, UpdateRecordRequest,
};
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::{
    BitableFieldSpec, BitableRecordBatch, BitableSchemaSpec, BitableSyncSummary, BitableTableSpec,
    BitableUpsertSummary, DDTClient, Error,
};

const FEISHU_API_BASE: &str = "https://open.feishu.cn";

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    code: i64,
    msg: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct ApiStatus {
    code: i64,
    msg: String,
}

#[derive(Debug, Clone)]
struct TableItem {
    table_id: String,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FieldItem {
    field_id: String,
    field_name: String,
    is_primary: bool,
}

#[derive(Debug, Clone)]
struct RecordItem {
    record_id: String,
    fields: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct FieldListData {
    #[serde(default)]
    items: Vec<FieldItem>,
}

pub async fn sync_bitable_schema(
    client: &DDTClient,
    spec: &BitableSchemaSpec,
) -> Result<BitableSyncSummary, Error> {
    spec.validate()?;

    let token = client.tenant_access_token().await?;
    let mut summary = BitableSyncSummary::default();
    for table in &spec.tables {
        let table_id = ensure_table(client, &token, &spec.app_token, table, &mut summary).await?;
        if !table.seed_records.is_empty() {
            let upsert_summary = upsert_records_for_table(
                client,
                &spec.app_token,
                &table_id,
                &table.name,
                &table.primary_field,
                &table.seed_records,
            )
            .await?;
            summary.merge_upsert(&upsert_summary);
        }
    }

    Ok(summary)
}

pub async fn apply_record_batch(
    client: &DDTClient,
    batch: &BitableRecordBatch,
) -> Result<BitableUpsertSummary, Error> {
    batch.validate()?;

    let _token = client.tenant_access_token().await?;
    let table_list = list_tables(client, &batch.app_token).await?;
    let table_by_name: HashMap<_, _> = table_list
        .into_iter()
        .map(|table| (table.name.clone(), table))
        .collect();

    let mut summary = BitableUpsertSummary::default();
    for table_batch in &batch.tables {
        let table = table_by_name
            .get(&table_batch.name)
            .ok_or_else(|| Error::TableNotFound {
                table: table_batch.name.clone(),
            })?;

        let table_summary = upsert_records_for_table(
            client,
            &batch.app_token,
            &table.table_id,
            &table_batch.name,
            &table_batch.key_field,
            &table_batch.records,
        )
        .await?;
        summary.created_records += table_summary.created_records;
        summary.updated_records += table_summary.updated_records;
    }

    Ok(summary)
}

async fn ensure_table(
    client: &DDTClient,
    token: &str,
    app_token: &str,
    table: &BitableTableSpec,
    summary: &mut BitableSyncSummary,
) -> Result<String, Error> {
    let existing_tables = list_tables(client, app_token).await?;
    let table_id = if let Some(existing) = existing_tables
        .into_iter()
        .find(|item| item.name == table.name)
    {
        existing.table_id
    } else {
        summary.created_tables += 1;
        create_table(client, app_token, &table.name).await?
    };

    let fields = list_fields(client, token, app_token, &table_id).await?;
    if !fields
        .iter()
        .any(|field| field.field_name == table.primary_field)
    {
        if let Some(primary_field) = fields.iter().find(|field| field.is_primary) {
            rename_primary_field(
                client,
                token,
                app_token,
                &table_id,
                &primary_field.field_id,
                &table.primary_field,
            )
            .await?;
        }
    }

    let fields = list_fields(client, token, app_token, &table_id).await?;
    for field in &table.fields {
        if !fields
            .iter()
            .any(|existing| existing.field_name == field.name)
        {
            create_field(client, token, app_token, &table_id, field).await?;
            summary.created_fields += 1;
        }
    }

    Ok(table_id)
}

async fn list_tables(client: &DDTClient, app_token: &str) -> Result<Vec<TableItem>, Error> {
    let request = ListTablesRequest::builder()
        .app_token(app_token)
        .page_size(100)
        .build();
    let response = client.lark.bitable.v1.app_table.list(request, None).await?;
    let data = response.data.ok_or(Error::MissingResponseData {
        context: "list_tables",
    })?;

    Ok(data
        .items
        .into_iter()
        .map(|item| TableItem {
            table_id: item.table_id,
            name: item.name,
        })
        .collect())
}

async fn list_fields(
    client: &DDTClient,
    token: &str,
    app_token: &str,
    table_id: &str,
) -> Result<Vec<FieldItem>, Error> {
    let path =
        format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/fields?page_size=100");
    let data: FieldListData =
        api_json(client, token, Method::GET, &path, None, "list_fields").await?;
    Ok(data.items)
}

async fn create_table(
    client: &DDTClient,
    app_token: &str,
    table_name: &str,
) -> Result<String, Error> {
    let request = CreateTableRequest::builder()
        .app_token(app_token)
        .table(TableData::new(table_name))
        .build();
    let response = client
        .lark
        .bitable
        .v1
        .app_table
        .create(request, None)
        .await?;
    let data = response.data.ok_or(Error::MissingResponseData {
        context: "create_table",
    })?;

    Ok(data.table_id)
}

async fn rename_primary_field(
    client: &DDTClient,
    token: &str,
    app_token: &str,
    table_id: &str,
    field_id: &str,
    new_name: &str,
) -> Result<(), Error> {
    let path =
        format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/fields/{field_id}");
    api_status(
        client,
        token,
        Method::PUT,
        &path,
        Some(json!({ "field_name": new_name, "type": 1 })),
        "rename_primary_field",
    )
    .await
}

async fn create_field(
    client: &DDTClient,
    token: &str,
    app_token: &str,
    table_id: &str,
    field: &BitableFieldSpec,
) -> Result<(), Error> {
    let path = format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/fields");
    let mut body = Map::new();
    body.insert("field_name".to_owned(), Value::String(field.name.clone()));
    body.insert(
        "type".to_owned(),
        Value::Number(field.api_type_code().into()),
    );
    if let Some(property) = field.property_json() {
        body.insert("property".to_owned(), property);
    }

    api_status(
        client,
        token,
        Method::POST,
        &path,
        Some(Value::Object(body)),
        "create_field",
    )
    .await
}

async fn upsert_records_for_table(
    client: &DDTClient,
    app_token: &str,
    table_id: &str,
    table_name: &str,
    key_field: &str,
    records: &[Map<String, Value>],
) -> Result<BitableUpsertSummary, Error> {
    let existing_records = list_records(client, app_token, table_id).await?;
    let mut existing_by_key = HashMap::new();

    for record in existing_records {
        if let Some(key) = record.fields.get(key_field).and_then(record_lookup_key) {
            existing_by_key.insert(key, record.record_id);
        }
    }

    let mut summary = BitableUpsertSummary::default();
    for record in records {
        let key = record
            .get(key_field)
            .and_then(record_lookup_key)
            .ok_or_else(|| Error::MissingRecordKey {
                table: table_name.to_owned(),
                key_field: key_field.to_owned(),
            })?;

        if let Some(record_id) = existing_by_key.get(&key) {
            update_record(client, app_token, table_id, record_id, record).await?;
            summary.updated_records += 1;
        } else {
            create_record(client, app_token, table_id, record).await?;
            summary.created_records += 1;
        }
    }

    Ok(summary)
}

async fn list_records(
    client: &DDTClient,
    app_token: &str,
    table_id: &str,
) -> Result<Vec<RecordItem>, Error> {
    let request = SearchRecordRequest::builder()
        .app_token(app_token)
        .table_id(table_id)
        .page_size(100)
        .build();
    let response = client
        .lark
        .bitable
        .v1
        .app_table_record
        .search(request, None)
        .await?;
    let data = response.data.ok_or(Error::MissingResponseData {
        context: "list_records",
    })?;

    Ok(data
        .items
        .into_iter()
        .map(|record| RecordItem {
            record_id: record.record_id.unwrap_or_default(),
            fields: record.fields.into_iter().collect(),
        })
        .collect())
}

async fn create_record(
    client: &DDTClient,
    app_token: &str,
    table_id: &str,
    fields: &Map<String, Value>,
) -> Result<(), Error> {
    let request = CreateRecordRequest::builder()
        .app_token(app_token)
        .table_id(table_id)
        .fields(Record::from_json(Value::Object(fields.clone())))
        .build();
    client
        .lark
        .bitable
        .v1
        .app_table_record
        .create(request, None)
        .await?;
    Ok(())
}

async fn update_record(
    client: &DDTClient,
    app_token: &str,
    table_id: &str,
    record_id: &str,
    fields: &Map<String, Value>,
) -> Result<(), Error> {
    let request = UpdateRecordRequest::builder()
        .app_token(app_token)
        .table_id(table_id)
        .record_id(record_id)
        .fields(Value::Object(fields.clone()))
        .build();
    let response = client
        .lark
        .bitable
        .v1
        .app_table_record
        .update(request, None)
        .await?;

    if response.success() {
        Ok(())
    } else {
        Err(Error::FeishuApi {
            context: "update_record",
            code: i64::from(response.code()),
            msg: response.msg().to_owned(),
        })
    }
}

fn record_lookup_key(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

impl DDTClient {
    async fn tenant_access_token(&self) -> Result<String, Error> {
        let token_manager = self.lark.config.token_manager.lock().await;
        token_manager
            .get_tenant_access_token(
                &self.lark.config,
                "",
                "",
                &self.lark.config.app_ticket_manager,
            )
            .await
            .map_err(Error::from)
    }
}

async fn api_json<T: DeserializeOwned>(
    client: &DDTClient,
    token: &str,
    method: Method,
    path: &str,
    body: Option<Value>,
    context: &'static str,
) -> Result<T, Error> {
    let response = send_request(client, token, method, path, body).await?;
    let payload: ApiEnvelope<T> = response.json().await?;
    unwrap_data(payload, context)
}

async fn api_status(
    client: &DDTClient,
    token: &str,
    method: Method,
    path: &str,
    body: Option<Value>,
    context: &'static str,
) -> Result<(), Error> {
    let response = send_request(client, token, method, path, body).await?;
    let payload: ApiStatus = response.json().await?;
    if payload.code != 0 {
        return Err(Error::FeishuApi {
            context,
            code: payload.code,
            msg: payload.msg,
        });
    }
    Ok(())
}

async fn send_request(
    client: &DDTClient,
    token: &str,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<reqwest::Response, Error> {
    let mut request = client
        .lark
        .config
        .http_client
        .request(method, format!("{FEISHU_API_BASE}{path}"))
        .bearer_auth(token);

    if let Some(body) = body {
        request = request.json(&body);
    }

    request
        .send()
        .await?
        .error_for_status()
        .map_err(Error::from)
}

fn unwrap_data<T>(payload: ApiEnvelope<T>, context: &'static str) -> Result<T, Error> {
    if payload.code != 0 {
        return Err(Error::FeishuApi {
            context,
            code: payload.code,
            msg: payload.msg,
        });
    }

    payload.data.ok_or(Error::MissingResponseData { context })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_lookup_key_supports_scalar_values() {
        assert_eq!(
            record_lookup_key(&Value::String("id".to_owned())),
            Some("id".to_owned())
        );
        assert_eq!(record_lookup_key(&json!(42)), Some("42".to_owned()));
        assert_eq!(record_lookup_key(&json!(true)), Some("true".to_owned()));
        assert_eq!(record_lookup_key(&json!(["x"])), None);
    }

    #[test]
    fn field_spec_builds_select_property_json() {
        let field = BitableFieldSpec {
            name: "family_boundary_style".to_owned(),
            field_type: super::super::BitableFieldType::SingleSelect,
            options: vec!["authoritative".to_owned(), "negotiated".to_owned()],
        };

        let property = field.property_json().expect("property should exist");
        let options = property["options"]
            .as_array()
            .expect("options should exist");
        assert_eq!(options.len(), 2);
        assert_eq!(options[0]["name"], "authoritative");
    }
}

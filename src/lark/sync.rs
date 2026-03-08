use std::collections::HashMap;

use open_lark::service::bitable::v1::Record;
use open_lark::service::bitable::v1::app_table::{
    CreateTableRequest, ListTablesRequest, TableData,
};
use open_lark::service::bitable::v1::app_table_record::{
    BatchDeleteRecordRequest, CreateRecordRequest, SearchRecordRequest, UpdateRecordRequest,
};
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::{
    BitableDuplicateNameCount, BitableFieldReference, BitableFieldSpec, BitableIntegrityReport,
    BitableLegacyFieldPurgeSummary, BitableLegacyFieldPurgeTableSummary, BitableRecordBatch,
    BitableRecordBatchTable, BitableRecordDedupSummary, BitableRecordDedupTableSummary,
    BitableSchemaSpec, BitableSyncSummary, BitableTableIntegrityReport, BitableTableSpec,
    BitableUpsertSummary, DDTClient, Error,
};

const FEISHU_API_BASE: &str = "https://open.feishu.cn";
const RECORD_DELETE_BATCH_SIZE: usize = 500;

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

pub async fn export_record_batch_from_spec(
    client: &DDTClient,
    spec: &BitableSchemaSpec,
) -> Result<BitableRecordBatch, Error> {
    spec.validate()?;

    let _token = client.tenant_access_token().await?;
    let table_list = list_tables(client, &spec.app_token).await?;
    let table_by_name: HashMap<_, _> = table_list
        .into_iter()
        .map(|table| (table.name.clone(), table))
        .collect();

    let mut tables = Vec::with_capacity(spec.tables.len());
    for table_spec in &spec.tables {
        let table = table_by_name
            .get(&table_spec.name)
            .ok_or_else(|| Error::TableNotFound {
                table: table_spec.name.clone(),
            })?;

        let mut records = list_records(client, &spec.app_token, &table.table_id).await?;
        records.sort_by(|left, right| {
            let left_key = left
                .fields
                .get(&table_spec.primary_field)
                .and_then(record_lookup_key)
                .unwrap_or_default();
            let right_key = right
                .fields
                .get(&table_spec.primary_field)
                .and_then(record_lookup_key)
                .unwrap_or_default();

            left_key
                .cmp(&right_key)
                .then_with(|| left.record_id.cmp(&right.record_id))
        });

        tables.push(BitableRecordBatchTable {
            name: table_spec.name.clone(),
            key_field: table_spec.primary_field.clone(),
            records: records.into_iter().map(|record| record.fields).collect(),
        });
    }

    Ok(BitableRecordBatch {
        app_token: spec.app_token.clone(),
        tables,
    })
}

pub async fn audit_record_batch_integrity(
    client: &DDTClient,
    batch: &BitableRecordBatch,
) -> Result<BitableIntegrityReport, Error> {
    batch.validate()?;

    let token = client.tenant_access_token().await?;
    let table_list = list_tables(client, &batch.app_token).await?;
    let table_by_name: HashMap<_, _> = table_list
        .into_iter()
        .map(|table| (table.name.clone(), table))
        .collect();

    let mut report = BitableIntegrityReport::default();
    for table_batch in &batch.tables {
        let table = table_by_name
            .get(&table_batch.name)
            .ok_or_else(|| Error::TableNotFound {
                table: table_batch.name.clone(),
            })?;

        let fields = list_fields(client, &token, &batch.app_token, &table.table_id).await?;
        let records = list_records(client, &batch.app_token, &table.table_id).await?;

        let table_report = BitableTableIntegrityReport {
            table: table_batch.name.clone(),
            table_id: table.table_id.clone(),
            key_field: table_batch.key_field.clone(),
            duplicate_fields: duplicate_fields_report(&fields),
            renamed_legacy_fields: legacy_field_report(&fields),
            duplicate_record_keys: duplicate_record_keys_report(&records, &table_batch.key_field),
            missing_key_records: missing_key_count(&records, &table_batch.key_field),
            record_count: records.len(),
            field_count: fields.len(),
        };

        if table_report.has_issues() {
            report.tables.push(table_report);
        }
    }

    Ok(report)
}

pub async fn purge_legacy_fields_from_spec(
    client: &DDTClient,
    spec: &BitableSchemaSpec,
    apply: bool,
    table_filter: &[String],
) -> Result<BitableLegacyFieldPurgeSummary, Error> {
    spec.validate()?;

    let token = client.tenant_access_token().await?;
    let table_list = list_tables(client, &spec.app_token).await?;
    let table_by_name: HashMap<_, _> = table_list
        .into_iter()
        .map(|table| (table.name.clone(), table))
        .collect();

    let mut summary = BitableLegacyFieldPurgeSummary {
        apply,
        ..BitableLegacyFieldPurgeSummary::default()
    };

    for table_spec in &spec.tables {
        if !table_filter.is_empty() && !table_filter.iter().any(|name| name == &table_spec.name) {
            continue;
        }

        let table = table_by_name
            .get(&table_spec.name)
            .ok_or_else(|| Error::TableNotFound {
                table: table_spec.name.clone(),
            })?;

        let fields = list_fields(client, &token, &spec.app_token, &table.table_id).await?;
        let legacy_fields = legacy_field_report(&fields);
        if legacy_fields.is_empty() {
            continue;
        }

        summary.total_candidate_fields += legacy_fields.len();
        let mut deleted_fields = 0;
        if apply {
            for field in &legacy_fields {
                delete_field(
                    client,
                    &token,
                    &spec.app_token,
                    &table.table_id,
                    &field.field_id,
                )
                .await?;
                deleted_fields += 1;
            }
            summary.total_deleted_fields += deleted_fields;
        }

        summary.tables.push(BitableLegacyFieldPurgeTableSummary {
            table: table_spec.name.clone(),
            table_id: table.table_id.clone(),
            legacy_fields,
            deleted_fields,
        });
    }

    Ok(summary)
}

pub async fn dedupe_record_batch(
    client: &DDTClient,
    batch: &BitableRecordBatch,
    apply: bool,
    table_filter: &[String],
) -> Result<BitableRecordDedupSummary, Error> {
    batch.validate()?;

    let table_list = list_tables(client, &batch.app_token).await?;
    let table_by_name: HashMap<_, _> = table_list
        .into_iter()
        .map(|table| (table.name.clone(), table))
        .collect();

    let mut summary = BitableRecordDedupSummary {
        apply,
        ..BitableRecordDedupSummary::default()
    };

    for table_batch in &batch.tables {
        if !table_filter.is_empty() && !table_filter.iter().any(|name| name == &table_batch.name) {
            continue;
        }

        let table = table_by_name
            .get(&table_batch.name)
            .ok_or_else(|| Error::TableNotFound {
                table: table_batch.name.clone(),
            })?;

        let records = list_records(client, &batch.app_token, &table.table_id).await?;
        let duplicate_record_keys = duplicate_record_keys_report(&records, &table_batch.key_field);
        let missing_key_records = missing_key_count(&records, &table_batch.key_field);
        let duplicate_records = duplicate_record_ids(&records, &table_batch.key_field);
        if duplicate_record_keys.is_empty() && missing_key_records == 0 {
            continue;
        }

        summary.total_candidate_records += duplicate_records.len();
        let mut deleted_records = 0;
        if apply && !duplicate_records.is_empty() {
            deleted_records = batch_delete_record_ids(
                client,
                &batch.app_token,
                &table.table_id,
                &duplicate_records,
            )
            .await?;
            summary.total_deleted_records += deleted_records;
        }

        summary.tables.push(BitableRecordDedupTableSummary {
            table: table_batch.name.clone(),
            table_id: table.table_id.clone(),
            key_field: table_batch.key_field.clone(),
            duplicate_record_keys,
            missing_key_records,
            duplicate_records: duplicate_records.len(),
            deleted_records,
        });
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
        if fields
            .iter()
            .any(|existing| existing.field_name == field.name)
        {
            continue;
        }

        if let Some(legacy_field) = fields.iter().find(|existing| {
            legacy_field_aliases(&table.name, &field.name)
                .iter()
                .any(|alias| existing.field_name == *alias)
        }) {
            rename_primary_field(
                client,
                token,
                app_token,
                &table_id,
                &legacy_field.field_id,
                &field.name,
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

fn legacy_field_aliases<'a>(table_name: &str, field_name: &str) -> &'a [&'a str] {
    match (table_name, field_name) {
        ("event_definitions", "stage_ids") => &["stage_tags"],
        ("event_definitions", "archetype_ids") => &["archetype_tags"],
        ("event_definitions", "country_ids") => &["country_tags"],
        ("event_definitions", "scene_context_ids") => &["scene_context_tags"],
        ("event_definitions", "default_option_set_ids") => &["default_option_sets"],
        _ => &[],
    }
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
    if let Some(data) = response.data {
        return Ok(data.table_id);
    }

    list_tables(client, app_token)
        .await?
        .into_iter()
        .find(|item| item.name == table_name)
        .map(|item| item.table_id)
        .ok_or(Error::MissingResponseData {
            context: "create_table",
        })
}

fn duplicate_fields_report(fields: &[FieldItem]) -> Vec<BitableDuplicateNameCount> {
    let mut counts = HashMap::new();
    for field in fields {
        *counts.entry(field.field_name.clone()).or_insert(0usize) += 1;
    }

    let mut duplicates = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, count)| BitableDuplicateNameCount { name, count })
        .collect::<Vec<_>>();
    duplicates.sort_by(|left, right| left.name.cmp(&right.name));
    duplicates
}

fn legacy_field_report(fields: &[FieldItem]) -> Vec<BitableFieldReference> {
    let mut legacy_fields = fields
        .iter()
        .filter(|field| field.field_name.contains("__legacy_dup_"))
        .map(|field| BitableFieldReference {
            field_id: field.field_id.clone(),
            field_name: field.field_name.clone(),
        })
        .collect::<Vec<_>>();
    legacy_fields.sort_by(|left, right| left.field_name.cmp(&right.field_name));
    legacy_fields
}

fn duplicate_record_keys_report(
    records: &[RecordItem],
    key_field: &str,
) -> Vec<BitableDuplicateNameCount> {
    let mut counts = HashMap::new();
    for record in records {
        if let Some(key) = record.fields.get(key_field).and_then(record_lookup_key) {
            *counts.entry(key).or_insert(0usize) += 1;
        }
    }

    let mut duplicates = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, count)| BitableDuplicateNameCount { name, count })
        .collect::<Vec<_>>();
    duplicates.sort_by(|left, right| left.name.cmp(&right.name));
    duplicates
}

fn missing_key_count(records: &[RecordItem], key_field: &str) -> usize {
    records
        .iter()
        .filter(|record| {
            record
                .fields
                .get(key_field)
                .and_then(record_lookup_key)
                .is_none()
        })
        .count()
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

async fn delete_field(
    client: &DDTClient,
    token: &str,
    app_token: &str,
    table_id: &str,
    field_id: &str,
) -> Result<(), Error> {
    let path =
        format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/fields/{field_id}");
    api_status(client, token, Method::DELETE, &path, None, "delete_field").await
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
    let mut records = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut request = SearchRecordRequest::builder()
            .app_token(app_token)
            .table_id(table_id)
            .page_size(100);
        if let Some(token) = &page_token {
            request = request.page_token(token);
        }

        let response = client
            .lark
            .bitable
            .v1
            .app_table_record
            .search(request.build(), None)
            .await?;
        let data = response.data.ok_or(Error::MissingResponseData {
            context: "list_records",
        })?;

        records.extend(data.items.into_iter().map(|record| RecordItem {
            record_id: record.record_id.unwrap_or_default(),
            fields: record.fields.into_iter().collect(),
        }));

        if !data.has_more {
            break;
        }

        page_token = data.page_token;
        if page_token.is_none() {
            break;
        }
    }

    Ok(records)
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

async fn batch_delete_record_ids(
    client: &DDTClient,
    app_token: &str,
    table_id: &str,
    record_ids: &[String],
) -> Result<usize, Error> {
    let mut deleted_records = 0;
    for chunk in record_ids.chunks(RECORD_DELETE_BATCH_SIZE) {
        let request = BatchDeleteRecordRequest::builder()
            .app_token(app_token)
            .table_id(table_id)
            .records(chunk.to_vec())
            .build();
        let response = client
            .lark
            .bitable
            .v1
            .app_table_record
            .batch_delete(request, None)
            .await?;
        let data = response.data.ok_or(Error::MissingResponseData {
            context: "batch_delete_record_ids",
        })?;
        deleted_records += data
            .records
            .into_iter()
            .filter(|record| record.deleted)
            .count();
    }
    Ok(deleted_records)
}

fn duplicate_record_ids(records: &[RecordItem], key_field: &str) -> Vec<String> {
    let mut grouped = HashMap::<String, Vec<String>>::new();
    for record in records {
        if let Some(key) = record.fields.get(key_field).and_then(record_lookup_key) {
            grouped
                .entry(key)
                .or_default()
                .push(record.record_id.clone());
        }
    }

    let mut duplicate_ids = Vec::new();
    for record_ids in grouped.into_values() {
        if record_ids.len() <= 1 {
            continue;
        }

        for record_id in record_ids.iter().take(record_ids.len() - 1) {
            duplicate_ids.push(record_id.clone());
        }
    }
    duplicate_ids
}

fn record_lookup_key(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(record_lookup_key_fragment)
                .collect::<String>();
            if text.is_empty() { None } else { Some(text) }
        }
        Value::Object(object) => record_lookup_key_fragment(&Value::Object(object.clone())),
        _ => None,
    }
}

fn record_lookup_key_fragment(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(record_lookup_key_fragment)
                .collect::<String>();
            if text.is_empty() { None } else { Some(text) }
        }
        Value::Object(object) => object.get("text").and_then(record_lookup_key_fragment),
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
        assert_eq!(record_lookup_key(&json!(["x"])), Some("x".to_owned()));
        assert_eq!(record_lookup_key(&json!({ "type": "text" })), None);
    }

    #[test]
    fn record_lookup_key_supports_feishu_text_runs() {
        assert_eq!(
            record_lookup_key(&json!([
                { "type": "text", "text": "jp" }
            ])),
            Some("jp".to_owned())
        );
        assert_eq!(
            record_lookup_key(&json!([
                { "type": "text", "text": "warm_" },
                { "type": "text", "text": "intro" }
            ])),
            Some("warm_intro".to_owned())
        );
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

    #[test]
    fn duplicate_record_key_report_counts_duplicates_and_missing_keys() {
        let records = vec![
            RecordItem {
                record_id: "rec1".to_owned(),
                fields: Map::from_iter([(String::from("id"), json!("alpha"))]),
            },
            RecordItem {
                record_id: "rec2".to_owned(),
                fields: Map::from_iter([(String::from("id"), json!([{"text": "alpha"}]))]),
            },
            RecordItem {
                record_id: "rec3".to_owned(),
                fields: Map::new(),
            },
        ];

        let duplicates = duplicate_record_keys_report(&records, "id");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].name, "alpha");
        assert_eq!(duplicates[0].count, 2);
        assert_eq!(missing_key_count(&records, "id"), 1);
    }

    #[test]
    fn event_definition_legacy_aliases_are_exposed() {
        assert_eq!(
            legacy_field_aliases("event_definitions", "stage_ids"),
            &["stage_tags"]
        );
        assert_eq!(
            legacy_field_aliases("event_definitions", "default_option_set_ids"),
            &["default_option_sets"]
        );
        assert!(legacy_field_aliases("countries", "archetype_id").is_empty());
    }
}

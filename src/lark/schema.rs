use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitableSchemaSpec {
    pub app_token: String,
    pub schema_version: String,
    #[serde(default)]
    pub tables: Vec<BitableTableSpec>,
}

impl BitableSchemaSpec {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let content = std::fs::read_to_string(path)?;
        let spec: Self = serde_json::from_str(&content)?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.app_token.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: "app_token must not be empty".to_owned(),
            });
        }

        if self.schema_version.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: "schema_version must not be empty".to_owned(),
            });
        }

        for table in &self.tables {
            table.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitableTableSpec {
    pub name: String,
    pub primary_field: String,
    #[serde(default)]
    pub fields: Vec<BitableFieldSpec>,
    #[serde(default)]
    pub seed_records: Vec<Map<String, Value>>,
}

impl BitableTableSpec {
    fn validate(&self) -> Result<(), Error> {
        if self.name.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: "table name must not be empty".to_owned(),
            });
        }

        if self.primary_field.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: format!("table {} has an empty primary_field", self.name),
            });
        }

        for field in &self.fields {
            field.validate(&self.name)?;
        }

        for record in &self.seed_records {
            if !record.contains_key(&self.primary_field) {
                return Err(Error::InvalidSpec {
                    context: format!(
                        "table {} has seed record without primary field {}",
                        self.name, self.primary_field
                    ),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitableFieldSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: BitableFieldType,
    #[serde(default)]
    pub options: Vec<String>,
}

impl BitableFieldSpec {
    fn validate(&self, table_name: &str) -> Result<(), Error> {
        if self.name.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: format!("table {} has field with empty name", table_name),
            });
        }

        if matches!(
            self.field_type,
            BitableFieldType::SingleSelect | BitableFieldType::MultiSelect
        ) && self.options.is_empty()
        {
            return Err(Error::InvalidSpec {
                context: format!(
                    "table {} field {} requires non-empty options",
                    table_name, self.name
                ),
            });
        }

        Ok(())
    }

    pub fn api_type_code(&self) -> i64 {
        self.field_type.api_type_code()
    }

    pub fn property_json(&self) -> Option<Value> {
        match self.field_type {
            BitableFieldType::SingleSelect | BitableFieldType::MultiSelect => Some(json!({
                "options": self.options.iter().map(|name| json!({ "name": name })).collect::<Vec<_>>()
            })),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BitableFieldType {
    Text,
    Number,
    SingleSelect,
    MultiSelect,
    Date,
    Checkbox,
}

impl BitableFieldType {
    pub fn api_type_code(self) -> i64 {
        match self {
            Self::Text => 1,
            Self::Number => 2,
            Self::SingleSelect => 3,
            Self::MultiSelect => 4,
            Self::Date => 5,
            Self::Checkbox => 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitableRecordBatch {
    pub app_token: String,
    #[serde(default)]
    pub tables: Vec<BitableRecordBatchTable>,
}

impl BitableRecordBatch {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let content = std::fs::read_to_string(path)?;
        let batch: Self = serde_json::from_str(&content)?;
        batch.validate()?;
        Ok(batch)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.app_token.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: "record batch app_token must not be empty".to_owned(),
            });
        }

        for table in &self.tables {
            table.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitableRecordBatchTable {
    pub name: String,
    pub key_field: String,
    #[serde(default)]
    pub records: Vec<Map<String, Value>>,
}

impl BitableRecordBatchTable {
    fn validate(&self) -> Result<(), Error> {
        if self.name.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: "record batch table name must not be empty".to_owned(),
            });
        }

        if self.key_field.trim().is_empty() {
            return Err(Error::InvalidSpec {
                context: format!("record batch table {} has empty key_field", self.name),
            });
        }

        for record in &self.records {
            if !record.contains_key(&self.key_field) {
                return Err(Error::MissingRecordKey {
                    table: self.name.clone(),
                    key_field: self.key_field.clone(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BitableSyncSummary {
    pub created_tables: usize,
    pub created_fields: usize,
    pub created_records: usize,
    pub updated_records: usize,
}

impl BitableSyncSummary {
    pub fn merge_upsert(&mut self, summary: &BitableUpsertSummary) {
        self.created_records += summary.created_records;
        self.updated_records += summary.updated_records;
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BitableUpsertSummary {
    pub created_records: usize,
    pub updated_records: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_select_requires_options() {
        let table = BitableTableSpec {
            name: "culture_archetypes".to_owned(),
            primary_field: "id".to_owned(),
            fields: vec![BitableFieldSpec {
                name: "family_boundary_style".to_owned(),
                field_type: BitableFieldType::SingleSelect,
                options: Vec::new(),
            }],
            seed_records: Vec::new(),
        };

        assert!(table.validate().is_err());
    }

    #[test]
    fn schema_spec_parses_and_validates() {
        let spec: BitableSchemaSpec = serde_json::from_str(
            r#"{
                "app_token":"app_token",
                "schema_version":"v1",
                "tables":[
                    {
                        "name":"export_versions",
                        "primary_field":"id",
                        "fields":[{"name":"notes","type":"text"}],
                        "seed_records":[{"id":"seed_1","notes":"ok"}]
                    }
                ]
            }"#,
        )
        .expect("valid spec");

        spec.validate().expect("spec should validate");
    }
}

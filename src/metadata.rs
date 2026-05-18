use std::collections::HashMap;

use sqlx::MySqlPool;

use crate::types::{AlarmLogFields, TagSample};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagMetadata {
    pub fault_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TagMetadataCache {
    by_node_id: HashMap<String, TagMetadata>,
    by_alias: HashMap<String, TagMetadata>,
    by_tag: HashMap<String, TagMetadata>,
    entries: usize,
}

impl TagMetadataCache {
    pub async fn load(pool: &MySqlPool) -> Result<Self, sqlx::Error> {
        let rows = sqlx::query_as::<_, (Option<String>, Option<String>, String, Option<String>)>(
            r#"
SELECT
  NULLIF(TRIM(t.node_id), '') AS node_id,
  NULLIF(TRIM(t.alias), '') AS alias,
  t.tag,
  NULLIF(TRIM(ft.fault_type_name), '') AS fault_type
FROM tag_catalog t
LEFT JOIN fault_type_catalog ft ON ft.id = t.fault_type_id
WHERE t.enabled = 1
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(Self::from_rows(rows))
    }

    pub fn from_rows<I>(rows: I) -> Self
    where
        I: IntoIterator<Item = (Option<String>, Option<String>, String, Option<String>)>,
    {
        let mut cache = Self::default();
        for (node_id, alias, tag, fault_type) in rows {
            cache.entries += 1;
            let metadata = TagMetadata {
                fault_type: clean_optional(fault_type),
            };
            if let Some(node_id) = clean_optional(node_id) {
                cache.by_node_id.insert(node_id, metadata.clone());
            }
            if let Some(alias) = clean_optional(alias) {
                cache.by_alias.insert(alias, metadata.clone());
            }
            if let Some(tag) = clean_optional(Some(tag)) {
                cache.by_tag.insert(tag, metadata);
            }
        }
        cache
    }

    pub fn lookup<'a>(
        &'a self,
        sample: &TagSample,
        fields: &AlarmLogFields,
    ) -> Option<&'a TagMetadata> {
        self.by_node_id
            .get(sample.node_id.trim())
            .or_else(|| self.by_alias.get(sample.alias.trim()))
            .or_else(|| self.by_tag.get(fields.tag.trim()))
            .or_else(|| self.by_tag.get(sample.tag_name().trim()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries == 0
    }

    pub fn len(&self) -> usize {
        self.entries
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

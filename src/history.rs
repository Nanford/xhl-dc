use chrono::{Datelike, NaiveDate, NaiveDateTime};
use thiserror::Error;

use crate::sink::{validate_mysql_identifier, SinkError};

#[derive(Debug, Error)]
pub enum HistoryQueryError {
    #[error("history query start time must be before end time")]
    EmptyTimeRange,
    #[error("history query selected no physical tables")]
    NoTables,
    #[error("invalid history table name: {0}")]
    InvalidTable(#[from] SinkError),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlarmHistoryFilter {
    pub location: Option<String>,
    pub device: Option<String>,
    pub device_id: Option<String>,
    pub fault_type: Option<String>,
    pub tag_value: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlarmHistoryQueryPlan {
    pub tables: Vec<String>,
    pub sql: String,
}

pub fn archive_table_name(base_table: &str, month: NaiveDate) -> Result<String, HistoryQueryError> {
    validate_mysql_identifier(base_table)?;
    let table = format!("{}_{:04}{:02}", base_table, month.year(), month.month());
    validate_mysql_identifier(&table)?;
    Ok(table)
}

pub fn select_history_tables(
    base_table: &str,
    start_at: NaiveDateTime,
    end_at: NaiveDateTime,
    current_month: NaiveDate,
) -> Result<Vec<String>, HistoryQueryError> {
    validate_mysql_identifier(base_table)?;
    if start_at >= end_at {
        return Err(HistoryQueryError::EmptyTimeRange);
    }

    let mut tables = Vec::new();
    let mut month = first_day_of_month(start_at.date());
    while month.and_hms_opt(0, 0, 0).expect("midnight is valid") < end_at {
        if month == current_month {
            tables.push(base_table.to_string());
        } else if month < current_month {
            tables.push(archive_table_name(base_table, month)?);
        }
        month = next_month(month);
    }

    tables.dedup();
    Ok(tables)
}

pub fn build_history_query(
    base_table: &str,
    start_at: NaiveDateTime,
    end_at: NaiveDateTime,
    current_month: NaiveDate,
    filter: &AlarmHistoryFilter,
) -> Result<AlarmHistoryQueryPlan, HistoryQueryError> {
    let tables = select_history_tables(base_table, start_at, end_at, current_month)?;
    if tables.is_empty() {
        return Err(HistoryQueryError::NoTables);
    }

    let branches = tables
        .iter()
        .map(|table| {
            format!(
                "SELECT id, location, device, device_id, node_id, alias, tag, fault_type, tag_state, tag_value, description, remark, create_at, update_at FROM `{table}` WHERE create_at >= ? AND create_at < ?{}",
                filter_sql(filter)
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    let sql = format!(
        "SELECT * FROM ({branches}) alarm_history ORDER BY create_at DESC, id DESC{}{}",
        limit_sql(filter),
        offset_sql(filter)
    );

    Ok(AlarmHistoryQueryPlan { tables, sql })
}

fn filter_sql(filter: &AlarmHistoryFilter) -> String {
    let mut sql = String::new();
    if filter.location.is_some() {
        sql.push_str(" AND location = ?");
    }
    if filter.device.is_some() {
        sql.push_str(" AND device = ?");
    }
    if filter.device_id.is_some() {
        sql.push_str(" AND device_id = ?");
    }
    if filter.fault_type.is_some() {
        sql.push_str(" AND fault_type = ?");
    }
    if filter.tag_value.is_some() {
        sql.push_str(" AND tag_value = ?");
    }
    sql
}

fn limit_sql(filter: &AlarmHistoryFilter) -> String {
    filter
        .limit
        .map(|_| " LIMIT ?".to_string())
        .unwrap_or_default()
}

fn offset_sql(filter: &AlarmHistoryFilter) -> String {
    filter
        .offset
        .map(|_| " OFFSET ?".to_string())
        .unwrap_or_default()
}

fn first_day_of_month(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("existing month has a first day")
}

fn next_month(month: NaiveDate) -> NaiveDate {
    let (year, month) = if month.month() == 12 {
        (month.year() + 1, 1)
    } else {
        (month.year(), month.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1).expect("next month has a first day")
}

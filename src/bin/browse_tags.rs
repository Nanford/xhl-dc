use std::fs::File;
use std::io::{BufWriter, Write as IoWrite};
use std::sync::Arc;
use std::time::Duration;

use opcua::client::transport::TcpConnector;
use opcua::client::{ClientBuilder, IdentityToken, Session, SessionEventLoop};
use opcua::types::{
    AttributeId, BrowseDescription, BrowseDirection, BrowseResultMask, DataValue,
    MessageSecurityMode, NodeClass, NodeClassMask, NodeId, ReadValueId, ReferenceTypeId,
    TimestampsToReturn, Variant,
};

struct TagInfo {
    path: String,
    node_id: String,
    data_type: String,
    value: String,
}

const ALARM_KEYWORDS: &[&str] = &[
    "alarm",
    "fault",
    "error",
    "estop",
    "warning",
    "status",
    "ds",
    "gs",
    "totalfault",
    "drivefault",
];

impl TagInfo {
    fn is_alarm_related(&self) -> bool {
        let lower = self.path.to_lowercase();
        ALARM_KEYWORDS.iter().any(|kw| lower.contains(kw))
    }
}

#[tokio::main]
async fn main() {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "opc.tcp://127.0.0.1:49321".to_string());

    let max_depth: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    eprintln!("[INFO] browse_tags starting...");
    eprintln!("[INFO] endpoint: {}", endpoint);
    eprintln!("[INFO] max_depth: {}", max_depth);
    eprintln!("[INFO] full output => browse_output.txt");
    eprintln!("[INFO] alarm tags  => browse_alarm.txt");

    match run(&endpoint, max_depth).await {
        Ok(()) => eprintln!("[INFO] done."),
        Err(err) => eprintln!("[ERROR] {:#}", err),
    }

    eprintln!("[INFO] press Enter to exit...");
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);
}

async fn run(endpoint: &str, max_depth: usize) -> anyhow::Result<()> {
    println!("Connecting to {} ...", endpoint);

    let mut client = ClientBuilder::new()
        .application_name("Kepware Tag Browser")
        .application_uri("urn:KepwareTagBrowser")
        .create_sample_keypair(true)
        .trust_server_certs(true)
        .session_retry_limit(0)
        .client()
        .map_err(|e| anyhow::anyhow!("failed to build client: {}", e.join("; ")))?;

    println!("Discovering endpoints...");
    let endpoints = client
        .get_server_endpoints_from_url(endpoint)
        .await
        .map_err(|e| anyhow::anyhow!("endpoint discovery failed: {e}"))?;

    let target_ep = endpoints
        .iter()
        .find(|ep| ep.security_mode == MessageSecurityMode::None)
        .ok_or_else(|| anyhow::anyhow!("no endpoint with SecurityMode=None found"))?
        .clone();

    println!(
        "Using: {} (mode={:?})",
        target_ep.endpoint_url, target_ep.security_mode
    );

    let (session, event_loop): (Arc<Session>, SessionEventLoop<TcpConnector>) = client
        .connect_to_matching_endpoint(target_ep, IdentityToken::Anonymous)
        .await
        .map_err(|e| anyhow::anyhow!("session creation failed: {e}"))?;
    let _event_loop_handle = event_loop.spawn();

    println!("Waiting for connection (timeout 15s)...");
    tokio::time::timeout(Duration::from_secs(15), session.wait_for_connection())
        .await
        .map_err(|_| anyhow::anyhow!("connection timed out"))?;

    println!("Connected! Browsing (this may take a while)...\n");

    let full_file = File::create("browse_output.txt")
        .map_err(|e| anyhow::anyhow!("cannot create browse_output.txt: {e}"))?;
    let mut full_writer = BufWriter::new(full_file);

    let mut tags: Vec<TagInfo> = Vec::new();
    let objects_node = NodeId::objects_folder_id();
    browse_recursive(
        &session,
        &objects_node,
        String::new(),
        0,
        max_depth,
        &mut tags,
        &mut full_writer,
    )
    .await?;
    full_writer.flush()?;

    let alarm_file = File::create("browse_alarm.txt")
        .map_err(|e| anyhow::anyhow!("cannot create browse_alarm.txt: {e}"))?;
    let mut alarm_writer = BufWriter::new(alarm_file);

    let alarm_tags: Vec<&TagInfo> = tags.iter().filter(|t| t.is_alarm_related()).collect();

    writeln!(alarm_writer, "# Alarm/Fault/Status related tags")?;
    writeln!(alarm_writer, "# Total tags scanned: {}", tags.len())?;
    writeln!(alarm_writer, "# Alarm tags found: {}", alarm_tags.len())?;
    writeln!(alarm_writer, "#")?;
    writeln!(alarm_writer, "# Format: path | node_id | type | value")?;
    writeln!(alarm_writer, "{}", "=".repeat(100))?;

    let mut current_group = String::new();
    for tag in &alarm_tags {
        let parts: Vec<&str> = tag.path.split('.').collect();
        let group = if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            parts[0].to_string()
        };
        if group != current_group {
            writeln!(alarm_writer)?;
            writeln!(alarm_writer, "--- {} ---", group)?;
            current_group = group;
        }
        writeln!(
            alarm_writer,
            "[TAG] {}  |  node_id={}  |  type={}  |  value={}",
            tag.path, tag.node_id, tag.data_type, tag.value
        )?;
    }
    alarm_writer.flush()?;

    println!("============================================================");
    println!("Total tags scanned: {}", tags.len());
    println!("Alarm/fault/status tags: {}", alarm_tags.len());
    println!();
    println!("Files written:");
    println!("  browse_output.txt  - all {} tags (full dump)", tags.len());
    println!(
        "  browse_alarm.txt   - {} alarm/fault/status tags only",
        alarm_tags.len()
    );
    println!();

    let mut top_dirs = std::collections::HashMap::new();
    for tag in &tags {
        let parts: Vec<&str> = tag.path.split('.').collect();
        let top = if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            parts[0].to_string()
        };
        *top_dirs.entry(top).or_insert(0u32) += 1;
    }
    println!("Top-level groups:");
    let mut sorted: Vec<_> = top_dirs.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (group, count) in &sorted {
        println!("  {}: {} tags", group, count);
    }

    println!();
    println!("Data types:");
    let mut type_counts = std::collections::HashMap::new();
    for tag in &tags {
        *type_counts.entry(tag.data_type.clone()).or_insert(0u32) += 1;
    }
    let mut sorted: Vec<_> = type_counts.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (dt, count) in sorted {
        println!("  {}: {}", dt, count);
    }

    let _ = session.disconnect().await;
    Ok(())
}

fn browse_recursive<'a>(
    session: &'a Session,
    node_id: &'a NodeId,
    path: String,
    depth: usize,
    max_depth: usize,
    tags: &'a mut Vec<TagInfo>,
    writer: &'a mut BufWriter<File>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'a>> {
    Box::pin(async move {
        if depth > max_depth {
            return Ok(());
        }

        let browse_desc = BrowseDescription {
            node_id: node_id.clone(),
            browse_direction: BrowseDirection::Forward,
            reference_type_id: ReferenceTypeId::HierarchicalReferences.into(),
            include_subtypes: true,
            node_class_mask: NodeClassMask::all().bits(),
            result_mask: BrowseResultMask::All as u32,
        };

        let results = session
            .browse(&[browse_desc], 0, None)
            .await
            .map_err(|e| anyhow::anyhow!("browse failed: {e}"))?;

        let Some(result) = results.into_iter().next() else {
            return Ok(());
        };

        let refs = result.references.unwrap_or_default();

        for r in &refs {
            let name = r.browse_name.name.to_string();
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", path, name)
            };
            let child_id = r.node_id.node_id.clone();
            let ns = child_id.namespace;

            let is_variable = r.node_class == NodeClass::Variable;
            let is_object = r.node_class == NodeClass::Object;

            if is_variable && ns >= 2 {
                let dt_name = read_data_type_name(session, &child_id).await;
                let val_str = read_current_value(session, &child_id).await;

                let _ = writeln!(
                    writer,
                    "[TAG] {}  |  node_id={}  |  type={}  |  value={}",
                    child_path, child_id, dt_name, val_str
                );
                tags.push(TagInfo {
                    path: child_path,
                    node_id: child_id.to_string(),
                    data_type: dt_name,
                    value: val_str,
                });

                if tags.len().is_multiple_of(1000) {
                    eprint!("\r[INFO] scanned {} tags...", tags.len());
                }
            } else if is_object {
                let skip = name == "_System" || name == "_Statistics" || name == "_Hints";
                if skip {
                    continue;
                }
                let _ = writeln!(writer, "{}[DIR]  {}", "  ".repeat(depth), child_path);
                browse_recursive(
                    session,
                    &child_id,
                    child_path,
                    depth + 1,
                    max_depth,
                    tags,
                    writer,
                )
                .await?;
            }
        }

        Ok(())
    })
}

async fn read_current_value(session: &Session, node_id: &NodeId) -> String {
    let read_id = ReadValueId::from(node_id.clone());
    match session
        .read(&[read_id], TimestampsToReturn::Neither, 0.0)
        .await
    {
        Ok(values) => format_data_value(values.first()),
        Err(_) => "<read error>".to_string(),
    }
}

async fn read_data_type_name(session: &Session, node_id: &NodeId) -> String {
    let read_id = ReadValueId {
        node_id: node_id.clone(),
        attribute_id: AttributeId::Value as u32,
        ..Default::default()
    };
    match session
        .read(&[read_id], TimestampsToReturn::Neither, 0.0)
        .await
    {
        Ok(values) => {
            if let Some(dv) = values.first() {
                variant_type_name(&dv.value)
            } else {
                "?".to_string()
            }
        }
        Err(_) => "?".to_string(),
    }
}

fn variant_type_name(value: &Option<Variant>) -> String {
    match value {
        Some(Variant::Boolean(_)) => "Boolean",
        Some(Variant::SByte(_)) => "SByte",
        Some(Variant::Byte(_)) => "Byte",
        Some(Variant::Int16(_)) => "Int16",
        Some(Variant::UInt16(_)) => "UInt16",
        Some(Variant::Int32(_)) => "Int32",
        Some(Variant::UInt32(_)) => "UInt32",
        Some(Variant::Int64(_)) => "Int64",
        Some(Variant::UInt64(_)) => "UInt64",
        Some(Variant::Float(_)) => "Float",
        Some(Variant::Double(_)) => "Double",
        Some(Variant::String(_)) => "String",
        Some(Variant::ByteString(_)) => "ByteString",
        _ => "?",
    }
    .to_string()
}

fn format_data_value(dv: Option<&DataValue>) -> String {
    match dv {
        Some(dv) => match &dv.value {
            Some(v) => {
                let s = format!("{:?}", v);
                if s.len() > 60 {
                    format!("{}...", &s[..60])
                } else {
                    s
                }
            }
            None => "<null>".to_string(),
        },
        None => "<empty>".to_string(),
    }
}

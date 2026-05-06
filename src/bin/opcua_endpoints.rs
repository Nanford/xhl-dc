use anyhow::Context;
use opcua::client::ClientBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "opc.tcp://127.0.0.1:49320".to_string());
    let client = ClientBuilder::new()
        .application_name("Kepware Bridge Endpoint Probe")
        .application_uri("urn:KepwareBridgeEndpointProbe")
        .create_sample_keypair(true)
        .trust_server_certs(true)
        .client()
        .map_err(|errors| anyhow::anyhow!("failed to build OPC UA client: {}", errors.join("; ")))?;

    let endpoints = client
        .get_server_endpoints_from_url(endpoint.as_str())
        .await
        .with_context(|| format!("failed to discover endpoints from {endpoint}"))?;

    for discovered in endpoints {
        println!("endpoint_url: {}", discovered.endpoint_url);
        println!("security_policy_uri: {}", discovered.security_policy_uri);
        println!("security_mode: {:?}", discovered.security_mode);
        println!("security_level: {}", discovered.security_level);
        println!("user_tokens:");
        if let Some(tokens) = discovered.user_identity_tokens {
            for token in tokens {
                println!(
                    "  - policy_id: {}, token_type: {:?}, security_policy_uri: {}",
                    token.policy_id, token.token_type, token.security_policy_uri
                );
            }
        }
        println!();
    }

    Ok(())
}

#[cfg(feature = "ddt")]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!(
            "usage: cargo run --example sync_bitable_schema --features ddt -- <schema-spec.json>"
        );
        std::process::exit(2);
    };

    let client = godot_plugin::DDTClient::new_with_env()?;
    let summary = client.sync_bitable_schema_from_path(path).await?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

#[cfg(not(feature = "ddt"))]
fn main() {
    eprintln!("enable the ddt feature to run this example");
    std::process::exit(2);
}

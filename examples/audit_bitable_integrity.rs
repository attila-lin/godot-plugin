#[cfg(feature = "ddt")]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!(
            "usage: cargo run --example audit_bitable_integrity --features ddt -- <record-batch.json>"
        );
        std::process::exit(2);
    };

    let client = godot_plugin::DDTClient::new_with_env()?;
    let report = client.audit_record_batch_integrity_from_path(path).await?;

    if report.is_clean() {
        println!("{{\n  \"status\": \"ok\"\n}}");
        return Ok(());
    }

    println!("{}", serde_json::to_string_pretty(&report)?);
    std::process::exit(1);
}

#[cfg(not(feature = "ddt"))]
fn main() {
    eprintln!("enable the ddt feature to run this example");
    std::process::exit(2);
}

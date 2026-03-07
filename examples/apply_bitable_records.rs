#[cfg(feature = "ddt")]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!(
            "usage: cargo run --example apply_bitable_records --features ddt -- <record-batch.json>"
        );
        std::process::exit(2);
    };

    let client = godot_plugin::DDTClient::new_with_env()?;
    let summary = client.apply_record_batch_from_path(path).await?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

#[cfg(not(feature = "ddt"))]
fn main() {
    eprintln!("enable the ddt feature to run this example");
    std::process::exit(2);
}

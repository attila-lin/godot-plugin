#[cfg(feature = "ddt")]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!(
            "usage: cargo run --example dedupe_bitable_records --features ddt -- <record-batch.json> [--apply] [--table <name>]..."
        );
        std::process::exit(2);
    };

    let mut apply = false;
    let mut tables = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--apply" => apply = true,
            "--table" => {
                let Some(table_name) = args.next() else {
                    eprintln!("missing value for --table");
                    std::process::exit(2);
                };
                tables.push(table_name);
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let client = godot_plugin::DDTClient::new_with_env()?;
    let summary = client
        .dedupe_record_batch_from_path(path, apply, &tables)
        .await?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

#[cfg(not(feature = "ddt"))]
fn main() {
    eprintln!("enable the ddt feature to run this example");
    std::process::exit(2);
}

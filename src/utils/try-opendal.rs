use opendal::Operator;
use tracing::{error, info};

fn main() {
    bbkar::utils::logging::init_tracing(0);
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    if let Err(e) = rt.block_on(async_main()) {
        error!("{}", e);
        std::process::exit(1);
    }
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // --- FILL IN YOUR GCS CONFIG HERE ---
    // See https://opendal.apache.org/docs/services/gcs/ for all options
    let bucket = "test-bbkar"; // e.g., "my-bucket"
    let credential_path = "/home/mono/.config/gcloud/application_default_credentials.json"; // e.g., "{...}"
    // Optionally, set a root prefix inside the bucket
    let root = "/";

    // Build the Operator for GCS using the service builder approach
    let gcs = opendal::services::Gcs::default()
        .bucket(bucket)
        .root(root)
        .credential_path(credential_path);

    let op: Operator = Operator::new(gcs)?.finish();

    info!("Using GCS OP: {:?}", op);

    // Write a file to GCS
    let path = "hello.txt";
    let content = b"Hello, OpenDAL with GCS!";
    match op.write(path, content.as_slice()).await {
        Ok(_) => info!("Successfully wrote {} to GCS bucket {}", path, bucket),
        Err(e) => {
            error!("Failed to write file: {}", e);
            return Ok(());
        }
    }

    // Read the file back from GCS
    match op.read(path).await {
        Ok(data) => {
            let bytes = data.to_bytes();
            let text = String::from_utf8_lossy(&bytes);
            info!("Read from GCS: {}", text);
        }
        Err(e) => {
            error!("Failed to read file: {}", e);
        }
    }

    // Optionally, delete the file afterwards
    // op.delete(path).await.ok();
    
    Ok(())
}

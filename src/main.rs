use banking::{TransactionRecord, TransactionResolver};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args();
    args.next(); // Skip the bin name
    let file_path = match args.next() {
        Some(path) => path,
        None => {
            return Err("The initial thing should be ok")?;
        }
    };

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_path(file_path)?;

    let iter = rdr.deserialize();
    // Just panic if something goes wrong
    let filtered_iter = iter.map(|i| i.unwrap());

    let mut transaction_resolver = TransactionResolver::new(filtered_iter);
    transaction_resolver.resolve().unwrap();

    Ok(())
}

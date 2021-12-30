use banking::{ClientAccount, DisputeAction, PaymentEngine, Transaction};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args();
    args.next(); // Skip the bin name
    let file_path = match args.next() {
        Some(path) => path,
        None => {
            return Err(
                "No path to a file has been found, please provide it as the first argument of this executable.".into(),
            );
        }
    };

    let csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_path(file_path)?;

    let csv_writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(std::io::stdout());

    process(csv_reader, csv_writer)?;

    Ok(())
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum RawRecordType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Deserialize, Debug)]
struct RawInputRecord {
    #[serde(rename = "type")]
    record_type: RawRecordType,
    client: u16,
    tx: u32,
    amount: Option<Decimal>,
}

#[derive(Serialize, Debug)]
struct RawOutputRecord {
    client: u16,
    available: Decimal,
    held: Decimal,
    total: Decimal,
    locked: bool,
}

impl<'a> From<&'a ClientAccount> for RawOutputRecord {
    fn from(c: &'a ClientAccount) -> Self {
        RawOutputRecord {
            client: c.id(),
            available: c.available(),
            held: c.held(),
            total: c.total(),
            locked: c.locked(),
        }
    }
}

fn process<R: std::io::Read, W: std::io::Write>(
    mut reader: csv::Reader<R>,
    mut writer: csv::Writer<W>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let iter = reader.deserialize();

    let mut payment_engine = PaymentEngine::new();

    for r in iter {
        // Due to internally tagged enums not being supported (https://github.com/BurntSushi/rust-csv/issues/211),
        // deserialize into an intermediate state before passing it along to the lib.
        let record: RawInputRecord = r?;
        match record.record_type {
            RawRecordType::Deposit => {
                let t = Transaction::Deposit {
                    client: record.client,
                    transaction_id: record.tx,
                    amount: record.amount.unwrap(),
                };
                payment_engine.add_transaction(t);
            }
            RawRecordType::Withdrawal => {
                let t = Transaction::Withdrawal {
                    client: record.client,
                    transaction_id: record.tx,
                    amount: record.amount.unwrap(),
                };
                payment_engine.add_transaction(t);
            }
            RawRecordType::Dispute => {
                let d = DisputeAction::Dispute {
                    client: record.client,
                    referenced_transaction_id: record.tx,
                };
                payment_engine.add_dispute_action(d);
            }
            RawRecordType::Resolve => {
                let d = DisputeAction::Resolve {
                    client: record.client,
                    referenced_transaction_id: record.tx,
                };
                payment_engine.add_dispute_action(d)
            }
            RawRecordType::Chargeback => {
                let d = DisputeAction::Chargeback {
                    client: record.client,
                    referenced_transaction_id: record.tx,
                };
                payment_engine.add_dispute_action(d);
            }
        }
    }

    payment_engine
        .get_all_client_states()
        .map(RawOutputRecord::from)
        .for_each(|r| {
            writer.serialize(r).unwrap();
        });

    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn single_row_with_header_and_leading_spaces() {
        let reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .trim(csv::Trim::All)
            .from_reader(
                &br#"type, client, tx, amount
deposit, 1, 1, 1.0"#[..],
            );

        let mut output: Vec<u8> = vec![];
        let writer = csv::Writer::from_writer(&mut output);

        process(reader, writer).unwrap();

        dbg!(std::str::from_utf8(&output[..]).unwrap());

        assert_eq!(
            output,
            b"client,available,held,total,locked\n1,1.0,0,1.0,false\n"
        )
    }

    #[test]
    fn single_row_with_header_and_no_leading_spaces() {
        let reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .trim(csv::Trim::All)
            .from_reader(
                &br#"type,client,tx,amount
deposit,1,1,1.0"#[..],
            );

        let mut output: Vec<u8> = vec![];
        let writer = csv::Writer::from_writer(&mut output);

        process(reader, writer).unwrap();

        dbg!(std::str::from_utf8(&output[..]).unwrap());

        assert_eq!(
            output,
            b"client,available,held,total,locked\n1,1.0,0,1.0,false\n"
        )
    }
}

use std::collections::HashMap;

use serde::Deserialize;

type Money = f64;

#[derive(Deserialize, Debug)]
pub struct TransactionRecord {
    #[serde(rename = "type")]
    transaction_type: TransactionType,
    client: u16,
    #[serde(rename = "tx")]
    transaction_id: u32,
    amount: Option<Money>, // TODO: check decimal places, replace with decimal crate or just left shift everything so we don't deal with decimals internally?
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

pub struct Client {
    id: u16,
    available: Money,
    held: Money,
    locked: bool,
}

pub struct TransactionResolver<T>
where
    T: Iterator<Item = TransactionRecord>,
{
    transaction_iter: T,
    state: HashMap<u16, Client>,
}

impl<T> TransactionResolver<T>
where
    T: Iterator<Item = TransactionRecord>,
{
    pub fn new(transaction_iter: T) -> Self {
        Self {
            transaction_iter,
            state: HashMap::new(),
        }
    }

    pub fn resolve(&mut self) -> Result<(), ()> {
        todo!();
    }

    pub fn state(&self) -> impl Iterator<Item = &Client> {
        self.state.values()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}

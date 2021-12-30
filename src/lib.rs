#![forbid(unsafe_code)]

use std::collections::HashMap;

use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub enum Transaction {
    Deposit {
        client: u16,
        transaction_id: u32,
        amount: Decimal,
    },
    Withdrawal {
        client: u16,
        transaction_id: u32,
        amount: Decimal,
    },
}

impl Transaction {
    fn get_client_id(&self) -> &u16 {
        match self {
            Transaction::Deposit { client, .. } => client,
            Transaction::Withdrawal { client, .. } => client,
        }
    }

    fn get_transaction_id(&self) -> &u32 {
        match self {
            Transaction::Deposit { transaction_id, .. } => transaction_id,
            Transaction::Withdrawal { transaction_id, .. } => transaction_id,
        }
    }
}

#[derive(Debug)]
pub enum DisputeAction {
    Dispute {
        client: u16,
        referenced_transaction_id: u32,
    },
    Resolve {
        client: u16,
        referenced_transaction_id: u32,
    },
    Chargeback {
        client: u16,
        referenced_transaction_id: u32,
    },
}

impl DisputeAction {
    fn get_client_id(&self) -> &u16 {
        match self {
            DisputeAction::Dispute { client, .. } => client,
            DisputeAction::Resolve { client, .. } => client,
            DisputeAction::Chargeback { client, .. } => client,
        }
    }

    fn get_referenced_transaction_id(&self) -> &u32 {
        match self {
            DisputeAction::Dispute {
                referenced_transaction_id: id,
                ..
            } => id,
            DisputeAction::Resolve {
                referenced_transaction_id: id,
                ..
            } => id,
            DisputeAction::Chargeback {
                referenced_transaction_id: id,
                ..
            } => id,
        }
    }
}

struct TransactionHistoryRecord {
    transaction: Transaction,
    state: TransactionState,
}

impl TransactionHistoryRecord {
    fn new(transaction: Transaction, accepted: bool) -> Self {
        Self {
            transaction,
            state: if accepted {
                TransactionState::Accepted
            } else {
                TransactionState::Rejected
            },
        }
    }
}

///
/// # State diagram
/// ```none
///
///     ┌───────┴────────┐
///     │                │
///     ▼                ▼
/// ┌────────┐      ┌────────┐
/// │Accepted│      │Rejected│
/// └───┬────┘      └────────┘
///     ▼
/// ┌────────┐    ┌────────────┐
/// │Disputed├──► │Chargebacked│
/// └───┬────┘    └────────────┘
///     ▼
/// ┌────────┐
/// │Resolved│
/// └────────┘
/// ```
enum TransactionState {
    Accepted,
    Rejected,
    Disputed,
    Resolved,
    Chargebacked,
}

pub struct ClientAccount {
    id: u16,
    /// A history of transactions and whether or not they were accepted.
    /// e.g. a withdrawal might fail due to insufficient funds.
    transaction_history: HashMap<u32, TransactionHistoryRecord>,
    dispute_history: Vec<DisputeAction>,
    available: Decimal,
    held: Decimal,
    locked: bool,
}

impl ClientAccount {
    pub fn new(id: u16) -> Self {
        Self {
            id,
            transaction_history: HashMap::new(),
            dispute_history: vec![],
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        }
    }

    /// Fails when trying to add a tranasaction that is not for this client, returning the passed in transaction.
    pub fn add_transaction(&mut self, transaction: Transaction) -> Result<(), Transaction> {
        if *transaction.get_client_id() != self.id {
            return Err(transaction);
        }

        if self.locked {
            // Prevent any transaction from having an effect when the client is locked.
            self.transaction_history.insert(
                *transaction.get_transaction_id(),
                TransactionHistoryRecord::new(transaction, false),
            );
            return Ok(());
        }

        match transaction {
            Transaction::Deposit { amount, .. } => {
                self.available += amount;
                self.transaction_history.insert(
                    *transaction.get_transaction_id(),
                    TransactionHistoryRecord::new(transaction, true),
                );
            }
            Transaction::Withdrawal { amount, .. } => {
                if self.withdrawal_amount_allowed(amount) {
                    self.available -= amount;
                    self.transaction_history.insert(
                        *transaction.get_transaction_id(),
                        TransactionHistoryRecord::new(transaction, true),
                    );
                } else {
                    self.transaction_history.insert(
                        *transaction.get_transaction_id(),
                        TransactionHistoryRecord::new(transaction, false),
                    );
                }
            }
        }

        Ok(())
    }

    /// Fails when trying to add an action for a client that is not this client. Returning the passed in dispute action.
    pub fn add_dispute_action(
        &mut self,
        dispute_action: DisputeAction,
    ) -> Result<(), DisputeAction> {
        if *dispute_action.get_client_id() != self.id {
            return Err(dispute_action);
        }

        if self.locked {
            // Prevent any transaction from having an effect when the client is locked.
            self.dispute_history.push(dispute_action);
            return Ok(());
        }

        let referenced_transaction_id = *dispute_action.get_referenced_transaction_id();

        let referenced_transaction =
            match self.transaction_history.get_mut(&referenced_transaction_id) {
                Some(t) => t,
                None => {
                    // Nothing to do, since the transaction doesn't exist (or it doesn't exist for this user!).
                    // Also don't store anything about it, since it's probably just a mistake.
                    return Ok(());
                }
            };

        match (&mut referenced_transaction.state, &dispute_action) {
            (state @ TransactionState::Accepted, DisputeAction::Dispute { .. }) => {
                match referenced_transaction.transaction {
                    Transaction::Deposit { amount, .. } => {
                        self.available -= amount;
                        self.held += amount;
                    }
                    Transaction::Withdrawal { .. } => {
                        // Don't do anything until the dispute is resolved.
                    }
                }
                self.dispute_history.push(dispute_action);
                *state = TransactionState::Disputed
            }
            (TransactionState::Rejected, DisputeAction::Dispute { .. }) => {
                // Disputing a rejected transaction is a NOOP.
            }
            (TransactionState::Disputed, DisputeAction::Dispute { .. }) => {
                // Don't do anything, disputing a disputed transaction is a NOOP.
            }
            (TransactionState::Resolved, DisputeAction::Dispute { .. }) => {
                // Disputing a resolved transaction is a NOOP, potentially we might want to user to be able to redispute this some amount of times?
            }
            (TransactionState::Chargebacked, DisputeAction::Dispute { .. }) => {
                // Disputing a chargebacked transaction is a NOOP, potentially we might want to user to be able to redispute this some amount of times?
            }

            (state @ TransactionState::Disputed, DisputeAction::Resolve { .. }) => {
                match referenced_transaction.transaction {
                    Transaction::Deposit { amount, .. } => {
                        self.available += amount;
                        self.held -= amount;
                    }
                    Transaction::Withdrawal { amount, .. } => {
                        self.available += amount;
                    }
                }
                self.dispute_history.push(dispute_action);
                *state = TransactionState::Resolved
            }
            (TransactionState::Accepted, DisputeAction::Resolve { .. }) => {
                // We cannot resolve something that is not disputed. Just ignore it.
            }
            (TransactionState::Rejected, DisputeAction::Resolve { .. }) => {
                // If it's rejected, we cannot resolve it.
            }
            (TransactionState::Resolved, DisputeAction::Resolve { .. }) => {
                // NOOP.
            }
            (TransactionState::Chargebacked, DisputeAction::Resolve { .. }) => {
                // It's already been chargebacked, resolving it is not possible..
            }

            (state @ TransactionState::Disputed, DisputeAction::Chargeback { .. }) => {
                match referenced_transaction.transaction {
                    Transaction::Deposit { amount, .. } => {
                        self.held -= amount;
                    }
                    Transaction::Withdrawal { .. } => {
                        // We didn't change anything about the funds for a witdrawal,
                        // so when we chargeback we don't have to do anything.
                    }
                }
                self.locked = true;
                self.dispute_history.push(dispute_action);
                *state = TransactionState::Chargebacked
            }
            (TransactionState::Accepted, DisputeAction::Chargeback { .. }) => {
                // Cannot chargeback something that is not disputed.
            }
            (TransactionState::Rejected, DisputeAction::Chargeback { .. }) => {
                // Can't change a rejected transaction
            }
            (TransactionState::Resolved, DisputeAction::Chargeback { .. }) => {
                // It's already resolved, we can't chargeback it after that
            }
            (TransactionState::Chargebacked, DisputeAction::Chargeback { .. }) => {
                // NOOP
            }
        }

        Ok(())
    }

    fn withdrawal_amount_allowed(&self, withdrawal_amount: Decimal) -> bool {
        self.available >= withdrawal_amount
    }

    pub fn id(&self) -> u16 {
        self.id
    }

    pub fn available(&self) -> Decimal {
        self.available
    }

    pub fn held(&self) -> Decimal {
        self.held
    }

    pub fn total(&self) -> Decimal {
        self.available + self.held
    }

    pub fn locked(&self) -> bool {
        self.locked
    }
}

pub struct PaymentEngine {
    state: HashMap<u16, ClientAccount>,
}

impl Default for PaymentEngine {
    fn default() -> Self {
        Self {
            state: HashMap::new(),
        }
    }
}

impl PaymentEngine {
    pub fn add_transaction(&mut self, transaction: Transaction) {
        let client = self
            .state
            .entry(*transaction.get_client_id())
            .or_insert_with(|| ClientAccount::new(*transaction.get_client_id()));
        // SAFETY:
        // `add_transaction` only returns an Err if we give it a transaction that does not belong to the client,
        // while we just ensured that we got the correct client.
        client
            .add_transaction(transaction)
            .expect("Retrieved the correct client.");
    }

    pub fn add_dispute_action(&mut self, dispute_action: DisputeAction) {
        let client = self
            .state
            .entry(*dispute_action.get_client_id())
            .or_insert_with(|| ClientAccount::new(*dispute_action.get_client_id()));
        // SAFETY:
        // `add_dispute_action` only returns an Err if we give it an action that does not belong to the client,
        // while we just ensured that we got the correct client.
        client
            .add_dispute_action(dispute_action)
            .expect("Retrieved the correct client.");
    }

    pub fn get_all_client_states(&self) -> impl Iterator<Item = &ClientAccount> {
        self.state.values()
    }

    pub fn get_client_state(&self, client_id: u16) -> Option<&ClientAccount> {
        self.state.get(&client_id)
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn no_transactions_no_problem() {
        let payment_engine = PaymentEngine::default();
        assert_eq!(payment_engine.get_all_client_states().count(), 0);
    }

    #[test]
    fn simple_deposit() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available,
            amount
        );
    }

    #[test]
    fn withdrawal_with_no_funds_available() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Withdrawal {
            client,
            transaction_id: 1,
            amount,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available,
            Decimal::ZERO
        );
    }

    #[test]
    fn withdrawal_after_deposit_for_same_amount() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_transaction(Transaction::Withdrawal {
            client,
            transaction_id: 2,
            amount,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available,
            Decimal::ZERO
        );
    }

    #[test]
    fn withdrawal_after_deposit_for_less_amount() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_transaction(Transaction::Withdrawal {
            client,
            transaction_id: 2,
            amount: amount - Decimal::ONE,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available,
            Decimal::ONE
        );
    }

    #[test]
    fn dispute_after_deposit_total_remains_same() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client,
            referenced_transaction_id: 1,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().total(),
            amount
        );
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available(),
            Decimal::ZERO
        );
    }

    #[test]
    fn chargeback_causes_locked_account() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client,
            referenced_transaction_id: 1,
        });
        payment_engine.add_dispute_action(DisputeAction::Chargeback {
            client,
            referenced_transaction_id: 1,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        let client_state = payment_engine.get_client_state(client).unwrap();

        assert!(client_state.locked());

        assert_eq!(client_state.total(), Decimal::ZERO);
    }

    #[test]
    fn dispute_resolve() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client,
            referenced_transaction_id: 1,
        });
        payment_engine.add_dispute_action(DisputeAction::Resolve {
            client,
            referenced_transaction_id: 1,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        let client_state = payment_engine.get_client_state(client).unwrap();

        assert!(!client_state.locked());
        assert_eq!(client_state.total(), amount);
        assert_eq!(client_state.available(), amount);
    }

    #[test]
    fn double_dispute_doesnt_hold_twice() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client,
            referenced_transaction_id: 1,
        });

        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client,
            referenced_transaction_id: 1,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().total(),
            amount
        );
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available(),
            Decimal::ZERO
        );
    }

    #[test]
    fn disputing_rejected_withdrawal_does_nothing() {
        let client = 1;
        let amount = dec!(2.0);
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client,
            transaction_id: 1,
            amount,
        });
        payment_engine.add_transaction(Transaction::Withdrawal {
            client,
            transaction_id: 2,
            amount: amount + Decimal::ONE,
        });
        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client,
            referenced_transaction_id: 2,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 1);
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().total(),
            amount
        );
        assert_eq!(
            payment_engine.get_client_state(client).unwrap().available(),
            amount
        );
    }

    #[test]
    fn multiple_clients() {
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client: 1,
            transaction_id: 1,
            amount: dec!(2.0),
        });
        payment_engine.add_transaction(Transaction::Deposit {
            client: 2,
            transaction_id: 2,
            amount: dec!(4.0),
        });
        payment_engine.add_transaction(Transaction::Deposit {
            client: 1,
            transaction_id: 3,
            amount: dec!(9.0),
        });
        payment_engine.add_transaction(Transaction::Withdrawal {
            client: 1,
            transaction_id: 4,
            amount: dec!(1.0),
        });
        payment_engine.add_transaction(Transaction::Withdrawal {
            client: 2,
            transaction_id: 5,
            amount: dec!(1.0),
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 2);
        assert_eq!(
            payment_engine.get_client_state(1).unwrap().available,
            dec!(10.0)
        );
        assert_eq!(
            payment_engine.get_client_state(2).unwrap().available,
            dec!(3.0)
        );
    }

    #[test]
    fn disputing_another_client_than_the_transaction_does_nothing() {
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client: 1,
            transaction_id: 1,
            amount: dec!(2.0),
        });

        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client: 2, // Another client than made the transaction!
            referenced_transaction_id: 1,
        });

        assert_eq!(payment_engine.get_all_client_states().count(), 2);
        assert_eq!(
            payment_engine.get_client_state(1).unwrap().available(),
            dec!(2.0)
        );
        assert_eq!(
            payment_engine.get_client_state(1).unwrap().held(),
            Decimal::ZERO
        );
        assert_eq!(payment_engine.get_client_state(1).unwrap().locked(), false);

        //The other client has been inserted as well!
        assert_eq!(
            payment_engine.get_client_state(2).unwrap().available(),
            Decimal::ZERO
        );
        assert_eq!(
            payment_engine.get_client_state(2).unwrap().held(),
            Decimal::ZERO
        );
        assert_eq!(payment_engine.get_client_state(2).unwrap().locked(), false);
    }

    #[test]
    fn dispute_withdrawal_and_resolve() {
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client: 1,
            transaction_id: 1,
            amount: dec!(2.0),
        });

        payment_engine.add_transaction(Transaction::Withdrawal {
            client: 1,
            transaction_id: 2,
            amount: dec!(1.0),
        });

        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client: 1,
            referenced_transaction_id: 2,
        });

        payment_engine.add_dispute_action(DisputeAction::Resolve {
            client: 1,
            referenced_transaction_id: 2,
        });

        assert_eq!(
            payment_engine.get_client_state(1).unwrap().held(),
            Decimal::ZERO
        );
        assert_eq!(
            payment_engine.get_client_state(1).unwrap().available(),
            dec!(2.0)
        );
        assert_eq!(payment_engine.get_client_state(1).unwrap().locked(), false);
    }

    #[test]
    fn dispute_withdrawal_and_charge_back() {
        let mut payment_engine = PaymentEngine::default();
        payment_engine.add_transaction(Transaction::Deposit {
            client: 1,
            transaction_id: 1,
            amount: dec!(2.0),
        });

        payment_engine.add_transaction(Transaction::Withdrawal {
            client: 1,
            transaction_id: 2,
            amount: dec!(1.0),
        });

        payment_engine.add_dispute_action(DisputeAction::Dispute {
            client: 1,
            referenced_transaction_id: 2,
        });

        payment_engine.add_dispute_action(DisputeAction::Chargeback {
            client: 1,
            referenced_transaction_id: 2,
        });

        assert_eq!(
            payment_engine.get_client_state(1).unwrap().held(),
            Decimal::ZERO
        );
        assert_eq!(
            payment_engine.get_client_state(1).unwrap().available(),
            dec!(1.0)
        );
        assert_eq!(payment_engine.get_client_state(1).unwrap().locked(), true);
    }
}

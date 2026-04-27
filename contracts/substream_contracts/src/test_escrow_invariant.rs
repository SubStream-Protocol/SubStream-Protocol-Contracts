#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

struct EscrowRng {
    state: u64,
}

impl EscrowRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.state
    }
}

#[test]
fn test_formal_escrow_non_negative_balance() {
    let env = Env::default();
    env.mock_all_auths();
    
    let mut rng = EscrowRng::new(142);

    for _ in 0..1000 {
        let admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token::Client::new(&env, &sac.address());
        let token_admin = token::StellarAssetClient::new(&env, &sac.address());

        let contract_id = env.register(SubStreamContract, ());
        let client = SubStreamContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        let merchant = Address::generate(&env);
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(
                &DataKey::MerchantRegistry(merchant.clone()),
                &MerchantStatus {
                    is_verified: true,
                    is_blacklisted: false,
                    verification_method: VerificationMethod::DAOApproval,
                    registered_at: 0,
                    last_verified: 0,
                    dao_approved: true,
                },
            );
        });

        let subscriber = Address::generate(&env);
        let initial_deposit = 1_000_000 * PRECISION_MULTIPLIER;
        token_admin.mint(&subscriber, &initial_deposit);

        let start: u64 = 1_000_000;
        env.ledger().set_timestamp(start);

        let rate = ((rng.next() % 100) + 1) as i128 * PRECISION_MULTIPLIER;
        client.subscribe(&subscriber, &merchant, &token.address, &initial_deposit, &rate, &None);

        let mut current_time = start;
        let mut active = true;

        for _ in 0..10 {
            // Chaos jump, deliberately intersecting exactly with trial boundaries
            if rng.next() % 5 == 0 {
                current_time = start + FREE_TRIAL_DURATION;
            } else {
                current_time += (rng.next() % 50_000) + 1;
            }
            env.ledger().set_timestamp(current_time);

            if !active { break; }

            if rng.next() % 5 == 0 {
                client.cancel(&subscriber, &merchant);
                active = false;
            } else {
                client.collect(&subscriber, &merchant);
            }

            let escrow_balance = token.balance(&contract_id);
            assert!(
                escrow_balance >= 0,
                "Escrow Non-Negative Invariant Violated! Insolvency detected. Balance: {}",
                escrow_balance
            );
        }
    }
}
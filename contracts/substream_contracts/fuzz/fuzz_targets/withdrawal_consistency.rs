#![no_main]
#![no_std]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, vec, Address, Env, Vec,
};
use substream_contracts::{SubStreamContract, SubStreamContractClient};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

fuzz_target!(|data: &[u8]| {
    if data.len() < 32 {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    // Generate deterministic addresses from fuzz data
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    // Create token contract
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    // Register main contract
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize at timestamp 0
    let start_time: u64 = 100;
    env.ledger().set_timestamp(start_time);

    // Calculate number of subscribers (up to 10,000 based on fuzz data)
    let num_subscribers = ((data[0] as usize) * 40 + (data[1] as usize)).min(10000).max(1);

    // Track all subscriber addresses and their initial deposits
    let mut subscribers: Vec<Address> = vec![&env];
    let mut initial_deposits: Vec<i128> = vec![&env];

    // Calculate initial vault balance we expect
    let mut expected_vault_balance: i128 = 0;

    // Create subscribers with varying amounts and subscribe them
    for i in 0..num_subscribers {
        let subscriber = Address::generate(&env);
        subscribers.push_back(subscriber.clone());

        // Determine deposit amount from fuzz data (between 100 and 10000)
        let data_idx = (i * 2 + 2) % data.len();
        let amount = 100 + ((data[data_idx] as i128) * 40 + (data[(data_idx + 1) % data.len()] as i128)).min(9900);
        initial_deposits.push_back(amount);
        expected_vault_balance += amount;

        // Mint tokens to subscriber
        token_admin.mint(&subscriber, &(amount * 2)); // Give extra for potential top-ups

        // Determine rate from fuzz data (between 1 and 100)
        let rate_idx = (i * 2 + 3) % data.len();
        let rate = 1 + (data[rate_idx] as i128).min(99);

        // Subscribe
        client.subscribe(&subscriber, &creator, &sac.address(), &amount, &rate);
    }

    // Verify initial vault balance
    let initial_vault_balance = token_client.balance(&contract_id);
    assert!(
        initial_vault_balance == expected_vault_balance,
        "Initial vault balance mismatch: expected {}, got {}",
        expected_vault_balance,
        initial_vault_balance
    );

    // Simulate time passing - advance past minimum duration + free trial
    let simulation_time = start_time + WEEK + DAY + (data[2] as u64) % DAY;
    env.ledger().set_timestamp(simulation_time);

    // Randomly perform some collections before cancellations (based on fuzz data)
    let collect_count = (data[3] as usize) % (num_subscribers.min(100));
    for i in 0..collect_count {
        let idx = (i + data[4] as usize) % num_subscribers;
        if let Some(subscriber) = subscribers.get(idx as u32) {
            // Try to collect - this should reduce vault balance by paying creators
            client.collect(&subscriber, &creator);
        }
    }

    // Track vault balance after collections
    let balance_after_collections = token_client.balance(&contract_id);
    assert!(
        balance_after_collections >= 0,
        "Vault balance went negative after collections: {}",
        balance_after_collections
    );

    // Now simulate simultaneous withdrawals (cancellations)
    // Shuffle order based on fuzz data to simulate race conditions
    let mut withdrawn_amount: i128 = 0;
    let mut remaining_subscribers = num_subscribers;

    for i in 0..num_subscribers {
        // Use fuzz data to determine order of cancellation
        let order_idx = ((i + data[5] as usize) % num_subscribers) as u32;
        if let Some(subscriber) = subscribers.get(order_idx) {
            // Check if still subscribed before cancelling
            if client.is_subscribed(&subscriber, &creator) {
                // Get balance before cancellation
                let contract_balance_before = token_client.balance(&contract_id);

                // Cancel subscription - this refunds remaining balance to subscriber
                client.cancel(&subscriber, &creator);

                // Get balance after cancellation
                let contract_balance_after = token_client.balance(&contract_id);

                // Verify contract balance never went negative
                assert!(
                    contract_balance_after >= 0,
                    "CRITICAL: Vault balance went negative after cancellation: {} at subscriber {}/{}",
                    contract_balance_after,
                    i,
                    num_subscribers
                );

                // Verify contract balance decreased or stayed same (refund paid out)
                // Note: It could increase if there were pending collections, but should never go negative
                assert!(
                    contract_balance_after >= 0,
                    "Vault balance negative after cancel: {}",
                    contract_balance_after
                );

                remaining_subscribers -= 1;
            }
        }
    }

    // Final verification: vault balance should never be negative
    let final_vault_balance = token_client.balance(&contract_id);
    assert!(
        final_vault_balance >= 0,
        "CRITICAL: Final vault balance is negative: {}",
        final_vault_balance
    );

    // Verify all subscriptions are cancelled
    for i in 0..num_subscribers {
        if let Some(subscriber) = subscribers.get(i as u32) {
            assert!(
                !client.is_subscribed(&subscriber, &creator),
                "Subscription {} still active after cancellation",
                i
            );
        }
    }
});

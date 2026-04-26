#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env, String};

use substream_contracts::{CancelVelocityMetrics, SubStreamContract, SubStreamContractClient};

const DAY: u64 = 24 * 60 * 60;
const SEP12_ISSUER: &str =
    "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2";

fn create_and_cancel_subscription(
    env: &Env,
    client: &SubStreamContractClient<'_>,
    token_admin: &token::StellarAssetClient<'_>,
    token_address: &Address,
    merchant: &Address,
    amount: i128,
    rate: i128,
) {
    let subscriber = Address::generate(env);
    token_admin.mint(&subscriber, &(amount.saturating_mul(2)));
    client.subscribe(
        &subscriber,
        merchant,
        token_address,
        &amount,
        &rate,
        &None,
    );
    client.cancel(&subscriber, merchant);
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let issuer = Address::from_string(&String::from_str(&env, SEP12_ISSUER));

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_merchant_with_kyc(&merchant, &soroban_sdk::vec![&env, 1u8], &issuer);

    let baseline_days = (data[0] % 30) as u64;
    let burst = 1 + (data[1] as usize % 64);
    let amount = 5 + (data[2] as i128 % 50);
    let rate = 1 + (data[3] as i128 % 5);

    for day in 0..baseline_days {
        env.ledger().set_timestamp(day.saturating_mul(DAY).saturating_add(10));
        create_and_cancel_subscription(
            &env,
            &client,
            &token_admin,
            &sac.address(),
            &merchant,
            amount,
            rate,
        );
    }

    env.ledger()
        .set_timestamp(baseline_days.saturating_mul(DAY).saturating_add(100));
    for _ in 0..burst {
        create_and_cancel_subscription(
            &env,
            &client,
            &token_admin,
            &sac.address(),
            &merchant,
            amount,
            rate,
        );
    }

    let metrics: CancelVelocityMetrics = client.get_cancel_velocity_metrics();
    assert_eq!(metrics.hourly_bucket_count, 24);
    assert_eq!(metrics.daily_bucket_count, 30);
    if metrics.rolling_24h_cancellations > metrics.anomaly_threshold {
        assert!(metrics.circuit_breaker_active);
        assert!(metrics.soft_pause_active);
    }
});

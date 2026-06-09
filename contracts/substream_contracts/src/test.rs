#[cfg(test)]
mod tests {
    use crate::{SubStreamContract, SubStreamContractClient};
    use soroban_sdk::{testutils::Address as _, vec, Address, Env};

    #[test]
    fn test_basic_subscription() {
        let env = Env::default();
        env.mock_all_auths();

        let subscriber = Address::generate(&env);
        let creator = Address::generate(&env);
        let token_admin = Address::generate(&env);

        let token_address = env.register_stellar_asset_contract_v2(token_admin.clone());

        let contract_id = env.register(SubStreamContract, ());
        let _client = SubStreamContractClient::new(&env, &contract_id);

        let creators = vec![&env, creator.clone()];
        let percentages = vec![&env, 100u32];
        let _ = (creators, percentages, subscriber, token_address);
    }
}

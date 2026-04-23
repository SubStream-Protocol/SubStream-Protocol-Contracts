#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Stream(Address, Address),
    TotalStreamed(Address, Address),
    CliffThreshold(Address),
    CreatorSubscribers(Address),
    CreatorMetadata(Address),
    ChannelPaused(Address),
    Escrow(Address, Address),
    Nullifier(Bytes),
    YieldConfig(Address),
    SLAStatus(Address),               // Merged from main
    UptimeOracleNonce(u64),           // Merged from main
    ContractAdmin,                    // Integrated for verify_creator
    VerifiedCreator(Address),
    UserReferrer(Address),
    ReferralTracker(Address, Address),
    MinimumRate(Address),
    CommunityGoal(Address),
    CurrentFlowRate(Address),
    AcceptedToken(Address),
    BlacklistedUser(Address, Address),
}
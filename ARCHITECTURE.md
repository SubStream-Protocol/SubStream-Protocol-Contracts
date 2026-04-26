# SubStream Protocol — Storage Architecture

This document maps every piece of contract state to its Soroban storage tier and explains the rationale. Auditors and integrators should use this as the authoritative reference.

---

## Storage Tiers

| Tier | Survives ledger eviction? | Cost | Use for |
|---|---|---|---|
| **Persistent** | Yes (with TTL bumps) | High | Critical state that must never be lost |
| **Temporary** | No (auto-evicted) | Low | Ephemeral state that can be recalculated or reset |
| **Instance** | Yes (tied to contract instance TTL) | Medium | Contract-wide config bumped on every call |

---

## Persistent Storage

These keys hold critical protocol state. Loss would be unrecoverable.

| DataKey | Type | Description |
|---|---|---|
| `ContractAdmin` | `Address` | Contract administrator |
| `VerifiedCreator(Address)` | `bool` | Creator verification status |
| `Subscription(Address, Address)` | `Subscription` | Active streaming subscription state |
| `BillingCycle(Address, Address)` | `BillingCycleInfo` | Pull-billing cycle state (subscriber, merchant) |
| `PlanRegistry(Address)` | `Vec<Plan>` | Merchant's subscription plans |
| `PendingMerchantPull(Address, Address)` | `PendingMerchantPullInfo` | Funds held in 48h dispute escrow |
| `TrialUsed(Address, Address)` | `bool` | Prevents trial reuse |
| `ActiveDispute(Address, Address)` | `bool` | Dispute lock flag |
| `DisputeRecord(u64)` | `DisputeRecord` | Full dispute record |
| `DisputeJurorKeys` | `Vec<BytesN<32>>` | Registered juror public keys |
| `NextDisputeId` | `u64` | Monotonic dispute ID counter |
| `MerchantRegistry(Address)` | `MerchantStatus` | Merchant KYC/verification status |
| `KYCCredential(Address)` | `KYCCredential` | SEP-12 KYC credential |
| `BlacklistedMerchant(Address)` | `bool` | Merchant blacklist flag |
| `BlacklistedUser(Address, Address)` | `bool` | Per-creator user blacklist |
| `DAOProposal(u64)` | `DAOProposal` | DAO governance proposal |
| `DAOVote(Address, u64)` | `DAOVote` | Individual DAO vote record |
| `RegistryUpdateProposal(u64)` | `RegistryUpdateProposal` | Timelocked registry update |
| `SecurityCouncilMember(Address)` | `SecurityCouncilMember` | Council member record |
| `SecurityCouncilVeto(Address, u64)` | `SecurityCouncilVeto` | Council veto record |
| `MerchantReferenceId(Address, Address)` | `String` | Web2 reference ID for webhook payloads (subscriber, merchant) |
| `MinimumRate(Address)` | `i128` | Creator's minimum subscription rate |
| `CommunityGoal(Address)` | `i128` | Creator's community funding goal |
| `MerchantMetrics(Address)` | `MerchantMetrics` | Aggregated merchant revenue metrics |
| `MerchantToS(Address)` | `MerchantToSAnchor` | Merchant Terms of Service IPFS anchor |
| `MerchantToSVersion(Address)` | `u32` | Current ToS version number |
| `SubscriptionToSSnapshot(Address, Address)` | `ToSSnapshot` | ToS version agreed at subscribe-time |
| `BuybackConfig` | `BuybackConfig` | DAO treasury buyback configuration |
| `BuybackNonce(u64)` | `Address` | Committed buyback nonce (anti-replay) |
| `FamilyVault(Address)` | `FamilyVaultConfig` | Family vault configuration |
| `VaultDelegate(Address, Address)` | `VaultDelegate` | Vault spending delegate |
| `VaultSubscription(Address, Address)` | `bool` | Vault subscription flag |
| `AffiliateConfig(Address)` | `AffiliateConfig` | Merchant affiliate program config |
| `AffiliateReferral(Address, Address)` | `AffiliateReferralInfo` | Per-affiliate referral tracking |
| `MerchantVacationMode(Address)` | `VacationModeStatus` | Merchant vacation mode state |
| `AcceptedToken(Address)` | `Address` | Creator's enforced stablecoin |
| `CliffThreshold(Address)` | `i128` | Creator's cliff access threshold |
| `UserContributed(Address, Address)` | `i128` | Lifetime fan contribution |
| `TopFans(Address)` | `Vec<TopFan>` | Top 50 fans by contribution |
| `CurrentFlowRate(Address)` | `i128` | Aggregated active flow rate |
| `UserReferrer(Address)` | `Address` | Referrer for a user |
| `ReferralTracker(Address, Address)` | `bool` | Referral registration flag |
| `SLAStatus(Address)` | `SLAStatus` | Creator SLA circuit breaker state |
| `UptimeOracleNonce(u64)` | `bool` | Oracle nonce anti-replay |
| `Nullifier(Bytes)` | `bool` | Anonymous subscription nullifier |
| `NullifierExpirationIndex(u64)` | `NullifierExpiration` | Nullifier expiry index for cleanup |
| `Escrow(Address, Address)` | `i128` | Dispute escrow balance |
| `YieldConfig(Address)` | `YieldConfig` | Yield strategy config |
| `DaoGrant(Address, Address)` | `DaoGrantStream` | DAO treasury grant stream |
| `CreatorMetadata(Address)` | `CreatorStats` | Creator earnings/fan stats |
| `CreatorAudience(Address, Address)` | `CreatorAudience` | Per-fan audience record |
| `ChannelPaused(Address)` | `bool` | Creator channel pause flag |

---

## Temporary Storage

These keys hold ephemeral state. If evicted, the contract recalculates or resets safely.

| DataKey | Type | Description | Eviction safety |
|---|---|---|---|
| `CancelVelocityHourlyBuckets` | `Vec<HourlyCancelBucket>` | Rolling 24-hour cancel counts | Resets to zero — circuit breaker becomes inactive, which is safe (conservative) |
| `CancelVelocityDailyBuckets` | `Vec<DailyCancelBucket>` | Rolling 30-day cancel counts | Same as above |
| `CancelVelocityBreakerState` | `VelocityCircuitBreakerState` | Circuit breaker active/paused flag | Resets to inactive — protocol resumes normally |
| `ReentrancyGuard` | `bool` | RAII reentrancy lock | Eviction during a call is impossible (same ledger); eviction between calls is safe (lock was already released) |
| `Subscription(Address, Address)` *(temporary copy)* | `Subscription` | Short-lived subscription cache used during collect | Recalculated from persistent copy on next access |

> **Security note on velocity eviction**: If `CancelVelocityBreakerState` is evicted while a soft-pause is active, the pause will be lifted. This is acceptable because: (1) the eviction TTL is set to 30 days, far longer than any realistic attack window; (2) the hourly/daily bucket data is also evicted, so the circuit breaker will not immediately re-trigger on stale data; (3) an admin can manually re-trigger via `reset_cancel_velocity_circuit_breaker`.

---

## Instance Storage

Instance storage is bumped on every contract invocation via `bump_instance_ttl`. It holds no application state directly — it keeps the contract instance alive on-chain.

---

## Migration Notes

**v1 → v2 (this release)**:
- `CancelVelocityHourlyBuckets`, `CancelVelocityDailyBuckets`, and `CancelVelocityBreakerState` were migrated from **Persistent** to **Temporary** storage. Existing persistent entries will be ignored (temporary reads return the default value). No migration transaction is required — the circuit breaker simply resets to inactive on first access after upgrade.

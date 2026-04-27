# SubStream Protocol — Disaster Recovery Runbook

> **"Break Glass in Case of Emergency"**
>
> This document is the authoritative emergency response manual for the SubStream
> Security Council and DAO key-holders. It is written for operators under high
> stress. Read the relevant section, execute the steps in order, and do not skip
> steps without explicit Security Council consensus.
>
> **Do not include private keys, seed phrases, or sensitive IP addresses in this
> document or any communication channel that references it.**

---

## Table of Contents

1. [Severity Levels](#1-severity-levels)
2. [Emergency Contacts & Multi-Sig Roster](#2-emergency-contacts--multi-sig-roster)
3. [Scenario A — Active Exploit / Fund Drain](#3-scenario-a--active-exploit--fund-drain)
4. [Scenario B — Cancel-Velocity Circuit Breaker Triggered](#4-scenario-b--cancel-velocity-circuit-breaker-triggered)
5. [Scenario C — Merchant KYC / Registry Compromise](#5-scenario-c--merchant-kyc--registry-compromise)
6. [Scenario D — Wasm Upgrade (Patch Deployment)](#6-scenario-d--wasm-upgrade-patch-deployment)
7. [Scenario E — State Migration V1 → V2](#7-scenario-e--state-migration-v1--v2)
8. [Scenario F — Oracle / Uptime Feed Failure](#8-scenario-f--oracle--uptime-feed-failure)
9. [Post-Incident Checklist](#9-post-incident-checklist)
10. [Glossary](#10-glossary)

---

## 1. Severity Levels

| Level | Description | Response Time | Action |
|-------|-------------|---------------|--------|
| **P0** | Active fund drain or exploit in progress | Immediate | Emergency pause + war room |
| **P1** | Vulnerability confirmed, not yet exploited | < 1 hour | Coordinate patch + timelock bypass |
| **P2** | Degraded service, no fund risk | < 4 hours | Investigate, patch via normal timelock |
| **P3** | Cosmetic / non-critical issue | < 24 hours | Normal governance process |

---

## 2. Emergency Contacts & Multi-Sig Roster

The Security Council consists of **5 members**. A minimum of **3 signatures** are
required to authorise any emergency action.

> Store the actual contact details and key-holder identities in a separate,
> access-controlled document (e.g., a hardware-encrypted vault or a private
> Signal group). **Never commit them to this repository.**

**Contract addresses (Stellar Testnet):**

```
Contract ID : CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L
Network     : Stellar Testnet
RPC URL     : https://soroban-testnet.stellar.org
```

---

## 3. Scenario A — Active Exploit / Fund Drain

### Symptoms
- Unusual token outflows from the contract address detected by monitoring.
- Subscriber balances dropping without corresponding `collect` or `cancel` events.
- Anomalous `Unsubscribed` events with zero refund amounts.

### Step 1 — Confirm the exploit

```bash
# Fetch recent contract events (last 200 ledgers)
stellar contract events \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --start-ledger $(stellar ledger current --network testnet | jq '.sequence - 200')
```

Look for unexpected `Unsubscribed`, `TipReceived`, or token `transfer` events
originating from the contract without a matching user-initiated transaction.

### Step 2 — Trigger the cancel-velocity soft pause

The cancel-velocity circuit breaker will block new subscriptions and top-ups.
If it has not auto-triggered, force it by calling `reset_cancel_velocity_circuit_breaker`
after manually setting the breaker state (requires admin key):

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <ADMIN_KEY_NAME> \
  -- reset_cancel_velocity_circuit_breaker \
  --admin <ADMIN_ADDRESS>
```

> **Note:** This resets the breaker. To *activate* a soft pause without a
> velocity spike, the Security Council must coordinate a Wasm upgrade that
> sets the breaker state directly (see Scenario D).

### Step 3 — Assemble the Security Council war room

- Notify all 5 Security Council members immediately.
- Establish a secure, real-time communication channel.
- Assign roles: **Incident Commander**, **On-Chain Executor**, **Communications Lead**.

### Step 4 — Freeze affected merchant accounts (if applicable)

If the exploit is tied to a specific merchant:

```bash
# Requires 3-of-5 Security Council signatures via propose_registry_update
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <COUNCIL_MEMBER_KEY> \
  -- propose_registry_update \
  --proposer <COUNCIL_MEMBER_ADDRESS> \
  --merchant <COMPROMISED_MERCHANT_ADDRESS> \
  --update_type BlacklistMerchant \
  --description "Emergency blacklist: active exploit" \
  --emergency_bypass true
```

Collect votes from 3 Security Council members:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <COUNCIL_MEMBER_KEY_N> \
  -- vote_registry_update \
  --voter <COUNCIL_MEMBER_ADDRESS_N> \
  --proposal_id <PROPOSAL_ID>
```

Execute after 3 votes:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <EXECUTOR_KEY> \
  -- execute_registry_update \
  --executor <EXECUTOR_ADDRESS> \
  --proposal_id <PROPOSAL_ID>
```

### Step 5 — Deploy emergency patch (see Scenario D)

---

## 4. Scenario B — Cancel-Velocity Circuit Breaker Triggered

### Symptoms
- `is_protocol_soft_paused` returns `true`.
- New subscriptions and top-ups are failing with `"protocol soft paused"`.

### Step 1 — Assess whether the trigger is legitimate

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  -- get_cancel_velocity_metrics
```

Compare `rolling_24h_cancellations` against `daily_average_30d`. A ratio above
`CANCEL_VELOCITY_MULTIPLIER` (5×) with at least `CANCEL_VELOCITY_MIN_TRIGGER`
(25) cancellations indicates a genuine attack.

### Step 2 — If legitimate attack: keep the pause active

Do not reset the breaker until the root cause is identified and patched.
Communicate the pause to users via official channels.

### Step 3 — If false positive: reset the breaker

After confirming no exploit is in progress:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <ADMIN_KEY_NAME> \
  -- reset_cancel_velocity_circuit_breaker \
  --admin <ADMIN_ADDRESS>
```

---

## 5. Scenario C — Merchant KYC / Registry Compromise

### Symptoms
- A verified merchant is found to be fraudulent or their KYC credentials were forged.
- A blacklisted merchant has re-registered under a new address.

### Step 1 — Immediately blacklist the merchant

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <DAO_MEMBER_KEY> \
  -- blacklist_merchant \
  --dao_member <DAO_MEMBER_ADDRESS> \
  --merchant <FRAUDULENT_MERCHANT_ADDRESS> \
  --reason "KYC credential forgery confirmed by Security Council"
```

### Step 2 — Notify affected subscribers

Query all active subscriptions to the merchant using off-chain indexing
(subgraph or event log scan), then notify subscribers through official channels.

### Step 3 — Revoke KYC credential at the issuer level

Contact the SEP-12 KYC issuer (`SEP12_KYC_ISSUER`) to revoke the credential
on-chain. This prevents the merchant from re-registering with the same hash.

---

## 6. Scenario D — Wasm Upgrade (Patch Deployment)

> A Wasm upgrade replaces the contract logic while preserving all persistent
> storage. This is the primary mechanism for deploying security patches.

### Prerequisites
- Patch has been audited by at least 2 Security Council members.
- New Wasm binary has been built and compressed.
- 3-of-5 Security Council signatures are available.

### Step 1 — Build and compress the patched Wasm

```bash
cd contracts/substream_contracts
make build-compressed
# Output: target/compressed/substream_contracts.optimized.wasm
```

### Step 2 — Upload the new Wasm to the Stellar network

```bash
stellar contract upload \
  --network testnet \
  --source-account <ADMIN_KEY_NAME> \
  --wasm contracts/substream_contracts/target/compressed/substream_contracts.optimized.wasm
# Note the returned WASM_HASH
```

### Step 3 — Propose the upgrade via timelock governance

For non-emergency upgrades (48-hour timelock):

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <COUNCIL_MEMBER_KEY> \
  -- propose_registry_update \
  --proposer <COUNCIL_MEMBER_ADDRESS> \
  --merchant <CONTRACT_ID> \
  --update_type WhitelistMerchant \
  --description "Wasm upgrade: patch CVE-XXXX-YYYY" \
  --emergency_bypass false
```

For P0 emergencies (bypass timelock):

```bash
# Set emergency_bypass to true — requires 3-of-5 Security Council votes
stellar contract invoke \
  ... \
  --emergency_bypass true
```

### Step 4 — Collect 3 Security Council votes

Each council member runs:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <COUNCIL_MEMBER_KEY_N> \
  -- vote_registry_update \
  --voter <COUNCIL_MEMBER_ADDRESS_N> \
  --proposal_id <PROPOSAL_ID>
```

### Step 5 — Execute the upgrade

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source-account <EXECUTOR_KEY> \
  -- execute_registry_update \
  --executor <EXECUTOR_ADDRESS> \
  --proposal_id <PROPOSAL_ID>
```

### Step 6 — Verify the upgrade

```bash
stellar contract info \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet
# Confirm the wasm_hash matches the uploaded binary
```

---

## 7. Scenario E — State Migration V1 → V2

> Use this procedure when a breaking storage schema change requires migrating
> subscriber state from the current contract to a new deployment.

### Step 1 — Deploy V2 contract

Deploy the new contract to a fresh address (do not upgrade in-place if the
storage schema is incompatible):

```bash
stellar contract deploy \
  --network testnet \
  --source-account <ADMIN_KEY_NAME> \
  --wasm contracts/substream_contracts/target/compressed/substream_contracts_v2.optimized.wasm
# Note the new V2_CONTRACT_ID
```

### Step 2 — Pause V1 to prevent new state changes

Trigger the cancel-velocity soft pause on V1 (see Scenario B, Step 3 in reverse)
or deploy a Wasm patch that hard-pauses all write operations.

### Step 3 — Export V1 state via event log

Use the subgraph or a custom indexer to export all active `Subscription` records:

```bash
# Example: fetch all Subscribed events that have no matching Unsubscribed event
stellar contract events \
  --id <V1_CONTRACT_ID> \
  --network testnet \
  --type contract \
  | jq '[.[] | select(.topic[0] == "Subscribed")]' > v1_active_subscriptions.json
```

### Step 4 — Re-initialise subscribers on V2

For each active subscription exported in Step 3, call the appropriate V2
initialisation function. This must be scripted and executed by the admin key:

```bash
# Pseudocode — adapt to actual V2 API
for sub in $(jq -c '.[]' v1_active_subscriptions.json); do
  SUBSCRIBER=$(echo $sub | jq -r '.subscriber')
  CREATOR=$(echo $sub | jq -r '.creator')
  # ... reconstruct subscription parameters
  stellar contract invoke \
    --id <V2_CONTRACT_ID> \
    --network testnet \
    --source-account <ADMIN_KEY_NAME> \
    -- initialize_subscription \
    --subscriber $SUBSCRIBER \
    --merchant $CREATOR \
    ...
done
```

### Step 5 — Verify solvency

After migration, confirm that the total token balance held by V2 equals the
sum of all migrated subscription balances:

```bash
stellar contract invoke \
  --id <V2_CONTRACT_ID> \
  --network testnet \
  -- dump_state_ledger
# Compare total_escrowed_tokens against token contract balance
```

### Step 6 — Announce migration and deprecate V1

Publish the V2 contract address through official channels. Update the frontend
and SDK to point to V2. Set V1 to read-only mode via a final Wasm patch.

---

## 8. Scenario F — Oracle / Uptime Feed Failure

### Symptoms
- SLA circuit breaker is not triggering despite confirmed downtime.
- `UptimeOracleNonce` entries are stale (older than `UPTIME_ORACLE_NONCE_TTL` = 24 hours).

### Step 1 — Identify the stale oracle

Check the last oracle submission timestamp:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  -- get_sla_status \
  --creator <CREATOR_ADDRESS>
```

### Step 2 — Restart the oracle service

The uptime oracle is an off-chain service. Restart it using the operator's
standard runbook. Ensure it submits a fresh `UptimeOraclePayload` with a new
nonce within the 24-hour validity window.

### Step 3 — If oracle is permanently unavailable

Propose a governance vote to disable the SLA circuit breaker for the affected
creator until a replacement oracle is deployed.

---

## 9. Post-Incident Checklist

After any P0 or P1 incident, complete the following within 48 hours:

- [ ] Write a post-mortem document (root cause, timeline, impact, remediation).
- [ ] Update this runbook with any new failure modes discovered.
- [ ] Rotate any keys that may have been exposed.
- [ ] Verify all Security Council members have reviewed the post-mortem.
- [ ] Publish a public disclosure (after 90-day responsible disclosure window if applicable).
- [ ] Update monitoring alerts to detect the same pattern earlier next time.
- [ ] Confirm the patched contract passes all CI tests and the doc-comment check.

---

## 10. Glossary

| Term | Definition |
|------|------------|
| **Security Council** | 5-member multi-sig body with veto and emergency powers |
| **DAO_MULTISIG_THRESHOLD** | Minimum 3 signatures required for governance actions |
| **Cancel-Velocity Circuit Breaker** | Auto-pause triggered when cancellation rate exceeds 5× the 30-day average |
| **Soft Pause** | Blocks new subscriptions and top-ups; existing streams continue |
| **Nullifier** | One-time-use token preventing ZK proof replay attacks |
| **DISPUTE_WINDOW_SEC** | 48-hour window during which a subscriber can dispute a merchant pull |
| **GRACE_PERIOD** | 24-hour window after balance exhaustion before a stream is terminated |
| **Wasm Upgrade** | In-place replacement of contract bytecode preserving storage |
| **Timelock** | 48-hour mandatory delay before a governance proposal can be executed |
| **Emergency Bypass** | Security Council override that skips the 48-hour timelock |

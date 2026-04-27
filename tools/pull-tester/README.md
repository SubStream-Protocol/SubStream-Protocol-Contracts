# SubStream Pull-Tester CLI

A local simulation tool for merchants integrating the SubStream pull-payment
protocol. Replicate the Soroban VM billing loop without a live testnet
connection — iterate in seconds, not block-times.

> **Safety:** All keys are MOCK keys. The tool has **no network I/O** and will
> never submit transactions to any Stellar network.

---

## Installation

```bash
cd tools/pull-tester
cargo build --release
# Binary: target/release/pull-tester
```

Or run directly:

```bash
cargo run -- <COMMAND> [OPTIONS]
```

---

## Quick Start

```bash
# 1. Register a billing plan for a merchant
pull-tester register-plan \
  --merchant GMERCHANT000000000000000000000000000000000000000000000 \
  --plan-id 1 \
  --name "Basic Plan" \
  --billing-amount 100 \
  --billing-cycle 2592000   # 30 days in seconds

# 2. Subscribe a user (funds their mock balance)
pull-tester subscribe \
  --subscriber GSUB0000000000000000000000000000000000000000000000000 \
  --merchant   GMERCHANT000000000000000000000000000000000000000000000 \
  --plan-id 1 \
  --initial-balance 500

# 3. Advance the simulated ledger past the billing date
pull-tester advance --seconds 2592001

# 4. Execute the merchant pull
pull-tester pull \
  --merchant   GMERCHANT000000000000000000000000000000000000000000000 \
  --subscriber GSUB0000000000000000000000000000000000000000000000000

# 5. Check subscription status
pull-tester status \
  --subscriber GSUB0000000000000000000000000000000000000000000000000
```

---

## Commands

| Command | Description |
|---------|-------------|
| `register-plan` | Register a billing plan for a merchant |
| `subscribe` | Subscribe a user to a merchant plan |
| `advance` | Advance the simulated ledger clock by N seconds |
| `pull` | Execute a merchant pull (collect one billing cycle) |
| `status` | Show subscription status for a subscriber |
| `ledger` | Show the current simulated timestamp |
| `reset` | Wipe all simulation state |
| `dump` | Print the full state as JSON |

---

## Output Format

All commands output **JSON** to stdout, making it easy to pipe into `jq` or
integrate with frontend/backend debugging scripts.

Example pull output:

```json
{
  "event": "SubscriptionBilled",
  "subscriber": "GSUB...",
  "merchant": "GMERCHANT...",
  "amount": 100,
  "billed_at": 3592001,
  "next_billing_date": 6184001,
  "receipt_hash": "mock-receipt-GMERCHA-GSUB0000-100-3592001",
  "cycle_number": 2592000,
  "subscriber_balance_after": 400,
  "merchant_balance_after": 100,
  "note": "MOCK — no network request made"
}
```

---

## Simulated Scenarios

### Trial → Active → Pull

```bash
pull-tester register-plan --merchant GM... --plan-id 2 --billing-amount 50 \
  --billing-cycle 2592000 --has-trial --trial-duration 604800
pull-tester subscribe --subscriber GS... --merchant GM... --plan-id 2
pull-tester advance --seconds 604801   # past trial
pull-tester advance --seconds 2592000  # past first billing cycle
pull-tester pull --merchant GM... --subscriber GS...
```

### Dunning (insufficient balance)

```bash
pull-tester subscribe --subscriber GS... --merchant GM... --plan-id 1 \
  --initial-balance 50   # less than billing_amount=100
pull-tester advance --seconds 2592001
pull-tester pull --merchant GM... --subscriber GS...
# → ERROR: insufficient balance — dunning started
```

---

## Running Tests

```bash
cargo test
```

---

## State File

Simulation state is persisted to `.pull-tester-state.json` in the current
directory. Run `pull-tester reset` to start fresh.

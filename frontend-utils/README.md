# SubStream Frontend Utilities

This directory contains reusable frontend components and utilities for the SubStream Protocol.

## Components

### SubscribeButtonWithFees

A feature-rich React component that displays subscription costs with transparent network fee breakdowns.

**Features:**
- ⚡ Real-time network fee calculation
- 💰 Complete cost breakdown (fees, deposit, rates)
- 🎁 7-day free trial information
- 📊 Estimated daily/weekly costs
- 💡 Comparison with traditional payment processors
- 🎨 Beautiful hover effects and animations
- 📱 Mobile-responsive design

**Usage:**

```tsx
import SubscribeButtonWithFees from './SubscribeButtonWithFees';

function MyComponent() {
  return (
    <SubscribeButtonWithFees
      creatorAddress="GCREATOR_ADDRESS_HERE"
      ratePerHour={1000000n} // 0.1 XLM per hour (in stroops)
      bufferAmount={500000000n} // 50 XLM buffer (in stroops)
      durationHours={24}
      onSubscribe={async () => {
        // Your subscription logic here
        console.log('Subscribed!');
      }}
    />
  );
}
```

**Props:**
- `creatorAddress` - Stellar address of the creator
- `ratePerHour` - Subscription rate in stroops per hour
- `bufferAmount` - Initial deposit amount in stroops
- `durationHours` - Minimum subscription duration (default: 24)
- `onSubscribe` - Callback function when subscribe is clicked

### FeeDisplay

A lightweight component for displaying network fees.

**Usage:**

```tsx
import { FeeDisplay } from './SubscribeButtonWithFees';

function TransactionRow() {
  return (
    <div>
      <span>Subscribe to Creator</span>
      <FeeDisplay transactionType="SUBSCRIBE" />
    </div>
  );
}
```

## Utilities

### feeEstimator.ts

Network fee calculation utilities for Stellar/Soroban transactions.

**Functions:**

#### `calculateNetworkFee(transactionType)`
Calculate estimated network fee for a transaction type.

```typescript
import { calculateNetworkFee } from './feeEstimator';

const subscribeFee = calculateNetworkFee('SUBSCRIBE');
const collectFee = calculateNetworkFee('COLLECT');
```

#### `formatXlm(amount)`
Format XLM amounts for display.

```typescript
import { formatXlm } from './feeEstimator';

const formatted = formatXlm(10000000n); // "1.0000 XLM"
```

#### `estimateSubscriptionCost(rate, duration, buffer)`
Get complete cost breakdown for a subscription.

```typescript
import { estimateSubscriptionCost } from './feeEstimator';

const breakdown = estimateSubscriptionCost(
  1000000n,  // 0.1 XLM/hour
  24,        // 24 hours
  500000000n // 50 XLM buffer
);

console.log(breakdown.networkFee);
console.log(breakdown.bufferAmount);
console.log(breakdown.estimatedFirstDay);
```

#### `getFeeDescription(transactionType)`
Get human-readable fee description.

```typescript
import { getFeeDescription } from './feeEstimator';

const desc = getFeeDescription('SUBSCRIBE');
// "Network fee: 0.0026 XLM (one-time, paid when creating subscription)"
```

#### `generateFeeTooltip(transactionType)`
Generate detailed tooltip content for UI.

#### `getFeeComparison(transactionType)`
Compare Stellar fees to traditional payment processors (PayPal, Stripe).

## Network Fee Estimates

Current estimates for different transaction types:

| Transaction | Estimated Fee (XLM) | USD Equivalent* |
|-------------|---------------------|-----------------|
| Subscribe   | ~0.0026 XLM         | ~$0.00013       |
| Collect     | ~0.0021 XLM         | ~$0.00011       |
| Cancel      | ~0.0018 XLM         | ~$0.00009       |
| Top Up      | ~0.0015 XLM         | ~$0.00008       |

*Based on 1 XLM = $0.05 USD

## Installation

When integrated into a Next.js project:

```bash
npm install @stellar/freighter-api stellar-sdk
```

## Configuration

The fee estimator uses current Stellar network parameters. For production use, you may want to:

1. Fetch real-time fee data from the network
2. Add buffer for fee fluctuations
3. Support multiple networks (testnet, futurenet, mainnet)

Example:

```typescript
// Fetch current base fee from network
const ledgerResponse = await server.ledgers().order('desc').limit(1).call();
const currentBaseFee = BigInt(ledgerResponse.records[0].baseFeeInStroops);
```

## Testing

Test the components in isolation:

```tsx
import { render, screen } from '@testing-library/react';
import SubscribeButtonWithFees from './SubscribeButtonWithFees';

test('displays network fee correctly', () => {
  render(
    <SubscribeButtonWithFees
      creatorAddress="GTEST..."
      ratePerHour={1000000n}
      bufferAmount={500000000n}
    />
  );
  
  expect(screen.getByText(/network fee/i)).toBeInTheDocument();
});
```

## Customization

### Styling

Override default styles using CSS modules or Tailwind config:

```css
/* In your CSS */
.subscribe-button-custom {
  /* Your custom styles */
}
```

### Fee Calculation

Extend the fee estimator for your use case:

```typescript
import { calculateNetworkFee } from './feeEstimator';

// Add custom transaction type
const CUSTOM_COMPLEXITY = {
  MY_CUSTOM_OPERATION: {
    instructions: 200000n,
    ledgerReads: 6n,
    ledgerWrites: 4n,
    metadataEntries: 3n,
  },
};

function calculateCustomFee() {
  // Your custom calculation
}
```

## Browser Support

- Chrome/Edge (latest)
- Firefox (latest)
- Safari (latest)

## Accessibility

Components follow WCAG guidelines:
- ✅ Keyboard navigation support
- ✅ Screen reader friendly
- ✅ Proper ARIA labels
- ✅ Focus indicators

## License

MIT

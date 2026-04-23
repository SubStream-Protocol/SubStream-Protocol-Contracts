/**
 * Network Fee Estimator for SubStream Protocol
 * 
 * This module provides utilities for calculating and displaying
 * network fees for Stellar/Soroban transactions.
 */

// Current base fee on Stellar network (in stroops, 1 stroop = 0.0000001 XLM)
const BASE_FEE_STROOPS = 100n;

// Typical operation fee (in stroops)
const OPERATION_FEE_STROOPS = 100n;

// Soroban-specific fees
const SOROBAN_INSTRUCTION_EXECUTION_FEE = 49n; // Per instruction
const SOROBAN_LEDGER_READ_FEE = 5768n; // Per entry read
const SOROBAN_LEDGER_WRITE_FEE = 57680n; // Per entry written
const SOROBAN_METADATA_FEE = 1000n; // Metadata overhead

// Transaction complexity estimates for different operations
const TRANSACTION_COMPLEXITY = {
  SUBSCRIBE: {
    instructions: 150000n,
    ledgerReads: 5n,
    ledgerWrites: 3n,
    metadataEntries: 2n,
  },
  COLLECT: {
    instructions: 100000n,
    ledgerReads: 4n,
    ledgerWrites: 2n,
    metadataEntries: 2n,
  },
  CANCEL: {
    instructions: 80000n,
    ledgerReads: 3n,
    ledgerWrites: 2n,
    metadataEntries: 1n,
  },
  TOP_UP: {
    instructions: 50000n,
    ledgerReads: 2n,
    ledgerWrites: 1n,
    metadataEntries: 1n,
  },
};

/**
 * Calculate network fee for a transaction
 * @param transactionType - Type of transaction (SUBSCRIBE, COLLECT, CANCEL, TOP_UP)
 * @returns Estimated fee in XLM
 */
export function calculateNetworkFee(transactionType: keyof typeof TRANSACTION_COMPLEXITY): bigint {
  const complexity = TRANSACTION_COMPLEXITY[transactionType];
  
  // Calculate instruction execution fee
  const instructionFee = complexity.instructions * SOROBAN_INSTRUCTION_EXECUTION_FEE;
  
  // Calculate ledger read fee
  const readFee = complexity.ledgerReads * SOROBAN_LEDGER_READ_FEE;
  
  // Calculate ledger write fee
  const writeFee = complexity.ledgerWrites * SOROBAN_LEDGER_WRITE_FEE;
  
  // Calculate metadata fee
  const metadataFee = complexity.metadataEntries * SOROBAN_METADATA_FEE;
  
  // Base transaction fee
  const baseTransactionFee = BASE_FEE_STROOPS * 100n; // Minimum 100 base units
  
  // Total fee in stroops
  const totalStroops = instructionFee + readFee + writeFee + metadataFee + baseTransactionFee + OPERATION_FEE_STROOPS;
  
  return stroopsToXlm(totalStroops);
}

/**
 * Convert stroops to XLM
 * @param stroops - Amount in stroops
 * @returns Amount in XLM
 */
export function stroopsToXlm(stroops: bigint): bigint {
  const STROOPS_PER_XLM = 10000000n;
  return stroops / STROOPS_PER_XLM;
}

/**
 * Convert XLM to stroops
 * @param xlm - Amount in XLM
 * @returns Amount in stroops
 */
export function xlmToStroops(xlm: bigint): bigint {
  const STROOPS_PER_XLM = 10000000n;
  return xlm * STROOPS_PER_XLM;
}

/**
 * Format XLM amount for display
 * @param xlm - Amount in XLM (as bigint representing whole units)
 * @returns Formatted string with XLM denomination
 */
export function formatXlm(xlm: bigint): string {
  const xlmNumber = Number(xlm);
  if (xlmNumber < 0.0001) {
    return `${xlmNumber.toFixed(7)} XLM`;
  } else if (xlmNumber < 0.01) {
    return `${xlmNumber.toFixed(5)} XLM`;
  } else {
    return `${xlmNumber.toFixed(4)} XLM`;
  }
}

/**
 * Get human-readable fee description
 * @param transactionType - Type of transaction
 * @returns Description string
 */
export function getFeeDescription(transactionType: keyof typeof TRANSACTION_COMPLEXITY): string {
  const fee = calculateNetworkFee(transactionType);
  const formattedFee = formatXlm(fee);
  
  const descriptions: Record<string, string> = {
    SUBSCRIBE: `Network fee: ${formattedFee} (one-time, paid when creating subscription)`,
    COLLECT: `Network fee: ${formattedFee} (paid when collecting earnings)`,
    CANCEL: `Network fee: ${formattedFee} (paid when canceling subscription)`,
    TOP_UP: `Network fee: ${formattedFee} (paid when adding funds)`,
  };
  
  return descriptions[transactionType] || `Network fee: ${formattedFee}`;
}

/**
 * Estimate total cost for subscribing
 * @param ratePerHour - Subscription rate in XLM per hour
 * @param durationHours - Duration in hours
 * @param bufferAmount - Initial buffer/deposit amount in XLM
 * @returns Object containing fee breakdown
 */
export interface SubscriptionCostBreakdown {
  networkFee: bigint;
  bufferAmount: bigint;
  estimatedHourlyRate: bigint;
  estimatedFirstDay: bigint;
  estimatedFirstWeek: bigint;
  trialPeriodDays: number;
}

export function estimateSubscriptionCost(
  ratePerHour: bigint,
  durationHours: number,
  bufferAmount: bigint
): SubscriptionCostBreakdown {
  const networkFee = calculateNetworkFee('SUBSCRIBE');
  const hoursInDay = 24n;
  const trialDays = 7;
  
  // Calculate estimated costs
  const estimatedFirstDay = ratePerHour * hoursInDay;
  const estimatedFirstWeek = ratePerHour * hoursInDay * BigInt(trialDays);
  
  return {
    networkFee,
    bufferAmount,
    estimatedHourlyRate: ratePerHour,
    estimatedFirstDay,
    estimatedFirstWeek,
    trialPeriodDays: trialDays,
  };
}

/**
 * Generate fee tooltip content
 * @param transactionType - Type of transaction
 * @returns HTML/JSX content for tooltip
 */
export function generateFeeTooltip(transactionType: keyof typeof TRANSACTION_COMPLEXITY): string {
  const fee = calculateNetworkFee(transactionType);
  const formattedFee = formatXlm(fee);
  
  let details = '';
  
  switch (transactionType) {
    case 'SUBSCRIBE':
      details = `
        • One-time network fee
        • Paid when creating subscription
        • Covers contract deployment & state updates
        • Refunded if transaction fails
      `;
      break;
    case 'COLLECT':
      details = `
        • Paid when withdrawing earnings
        • Deducted from collected amount
        • Much lower than traditional payment processors
      `;
      break;
    case 'CANCEL':
      details = `
        • Paid when canceling subscription
        • Refund processed automatically
        • Only charged after minimum duration
      `;
      break;
    case 'TOP_UP':
      details = `
        • Paid when adding funds to subscription
        • Instant processing
        • No hidden fees or charges
      `;
      break;
  }
  
  return `
Network Fee Breakdown:
━━━━━━━━━━━━━━━━━━━━━━
${formattedFee}
${details}
━━━━━━━━━━━━━━━━━━━━━━
💡 Tip: Fees are typically < $0.01 USD
  `.trim();
}

/**
 * Compare Stellar fees to traditional payment processors
 * @param transactionType - Type of transaction
 * @returns Comparison string
 */
export function getFeeComparison(transactionType: keyof typeof TRANSACTION_COMPLEXITY): string {
  const stellarFee = Number(calculateNetworkFee(transactionType));
  
  // Traditional payment processor fees (approximate)
  const paypalFee = 0.30 + (stellarFee * 100); // $0.30 + 2.9%
  const stripeFee = 0.30 + (stellarFee * 100); // $0.30 + 2.9%
  const creditCardFee = 0.25 + (stellarFee * 100); // $0.25 + 2-3%
  
  const savingsVsPaypal = ((paypalFee - stellarFee) / paypalFee * 100).toFixed(1);
  const savingsVsStripe = ((stripeFee - stellarFee) / stripeFee * 100).toFixed(1);
  
  return `
Save up to ${savingsVsPaypal}% vs PayPal
Save up to ${savingsVsStripe}% vs Stripe
No monthly fees, no chargebacks!
  `.trim();
}

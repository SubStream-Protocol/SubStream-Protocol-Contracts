'use client';

import { useState, useEffect } from 'react';
import { 
  calculateNetworkFee, 
  formatXlm, 
  estimateSubscriptionCost,
  getFeeDescription,
  generateFeeTooltip,
  type SubscriptionCostBreakdown
} from '../feeEstimator';

interface SubscribeButtonWithFeesProps {
  creatorAddress: string;
  ratePerHour: bigint; // in XLM
  bufferAmount: bigint; // in XLM
  durationHours?: number;
  onSubscribe?: () => void;
}

export default function SubscribeButtonWithFees({
  creatorAddress,
  ratePerHour,
  bufferAmount,
  durationHours = 24, // Default 24 hours minimum
  onSubscribe
}: SubscribeButtonWithFeesProps) {
  const [isHovering, setIsHovering] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const [costBreakdown, setCostBreakdown] = useState<SubscriptionCostBreakdown | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    // Calculate cost breakdown on mount
    const breakdown = estimateSubscriptionCost(ratePerHour, durationHours, bufferAmount);
    setCostBreakdown(breakdown);
  }, [ratePerHour, bufferAmount, durationHours]);

  const handleSubscribe = async () => {
    setIsLoading(true);
    try {
      if (onSubscribe) {
        await onSubscribe();
      } else {
        // Default subscription logic would go here
        console.log('Subscribing to:', creatorAddress);
        console.log('Rate:', ratePerHour, 'XLM/hour');
        console.log('Buffer:', bufferAmount, 'XLM');
      }
    } catch (error) {
      console.error('Subscription failed:', error);
    } finally {
      setIsLoading(false);
    }
  };

  if (!costBreakdown) {
    return <button disabled>Loading...</button>;
  }

  return (
    <div className="relative inline-block">
      {/* Main Subscribe Button */}
      <button
        onClick={handleSubscribe}
        disabled={isLoading}
        onMouseEnter={() => setIsHovering(true)}
        onMouseLeave={() => setIsHovering(false)}
        className={`
          bg-gradient-to-r from-blue-600 to-indigo-600 
          hover:from-blue-700 hover:to-indigo-700
          text-white font-bold py-3 px-6 rounded-lg
          shadow-lg hover:shadow-xl
          transform transition-all duration-200
          disabled:opacity-50 disabled:cursor-not-allowed
          flex items-center space-x-2
        `}
      >
        {isLoading ? (
          <>
            <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
            <span>Processing...</span>
          </>
        ) : (
          <>
            <span>🚀</span>
            <span>Subscribe Now</span>
          </>
        )}
      </button>

      {/* Fee Information Badge */}
      <div className="absolute -top-2 -right-2">
        <span className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-green-100 text-green-800">
          ⚡ &lt;${(Number(costBreakdown.networkFee) * 0.05).toFixed(2)} USD
        </span>
      </div>

      {/* Fee Details Tooltip/Popover */}
      {(isHovering || showDetails) && (
        <div 
          className="absolute z-50 bottom-full left-1/2 transform -translate-x-1/2 mb-2 w-80"
          onMouseEnter={() => setShowDetails(true)}
          onMouseLeave={() => setShowDetails(false)}
        >
          <div className="bg-white rounded-lg shadow-2xl border border-gray-200 p-4">
            {/* Header */}
            <div className="border-b border-gray-200 pb-2 mb-3">
              <h3 className="text-sm font-semibold text-gray-900">
                💰 Cost Breakdown
              </h3>
            </div>

            {/* Network Fee */}
            <div className="mb-3">
              <div className="flex justify-between items-center mb-1">
                <span className="text-xs text-gray-600">Network Fee</span>
                <span className="text-sm font-semibold text-blue-600">
                  {formatXlm(costBreakdown.networkFee)}
                </span>
              </div>
              <p className="text-xs text-gray-500">
                One-time Stellar network fee
              </p>
            </div>

            {/* Buffer/Deposit */}
            <div className="mb-3">
              <div className="flex justify-between items-center mb-1">
                <span className="text-xs text-gray-600">Initial Deposit</span>
                <span className="text-sm font-semibold text-blue-600">
                  {formatXlm(costBreakdown.bufferAmount)}
                </span>
              </div>
              <p className="text-xs text-gray-500">
                Your money, not a fee! Used for streaming payments
              </p>
            </div>

            {/* Subscription Rate */}
            <div className="mb-3">
              <div className="flex justify-between items-center mb-1">
                <span className="text-xs text-gray-600">Rate</span>
                <span className="text-sm font-semibold text-blue-600">
                  {formatXlm(costBreakdown.estimatedHourlyRate)}/hour
                </span>
              </div>
              <p className="text-xs text-gray-500">
                Pay-as-you-go streaming
              </p>
            </div>

            {/* Trial Period */}
            <div className="bg-green-50 rounded-md p-2 mb-3">
              <div className="flex items-start space-x-2">
                <span className="text-lg">🎁</span>
                <div>
                  <p className="text-xs font-semibold text-green-800">
                    {costBreakdown.trialPeriodDays}-Day Free Trial
                  </p>
                  <p className="text-xs text-green-600">
                    No charges for first {costBreakdown.trialPeriodDays} days!
                  </p>
                </div>
              </div>
            </div>

            {/* Estimated Costs */}
            <div className="border-t border-gray-200 pt-3 mt-3">
              <p className="text-xs font-semibold text-gray-700 mb-2">
                Estimated Costs (after trial):
              </p>
              <div className="space-y-1">
                <div className="flex justify-between text-xs">
                  <span className="text-gray-600">Per day:</span>
                  <span className="font-medium text-gray-900">
                    {formatXlm(costBreakdown.estimatedFirstDay / BigInt(costBreakdown.trialPeriodDays))}
                  </span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-gray-600">Per week:</span>
                  <span className="font-medium text-gray-900">
                    {formatXlm(costBreakdown.estimatedFirstWeek / BigInt(costBreakdown.trialPeriodDays))}
                  </span>
                </div>
              </div>
            </div>

            {/* Total Upfront */}
            <div className="bg-blue-50 rounded-md p-3 mt-3">
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-blue-900">Total Upfront:</span>
                <span className="text-lg font-bold text-blue-600">
                  {formatXlm(costBreakdown.networkFee + costBreakdown.bufferAmount)}
                </span>
              </div>
              <p className="text-xs text-blue-700 mt-1">
                Includes network fee + your deposit
              </p>
            </div>

            {/* Comparison */}
            <div className="border-t border-gray-200 pt-3 mt-3">
              <p className="text-xs text-gray-500 text-center">
                💡 Save up to 95% vs traditional payment processors
              </p>
            </div>

            {/* Learn More Link */}
            <div className="mt-3 text-center">
              <a 
                href="/docs/fees" 
                target="_blank"
                rel="noopener noreferrer"
                className="text-xs text-blue-600 hover:text-blue-800 underline"
              >
                Learn more about fees
              </a>
            </div>
          </div>
          
          {/* Arrow */}
          <div className="absolute top-full left-1/2 transform -translate-x-1/2 -mt-2">
            <div className="border-l-8 border-r-8 border-t-8 border-l-transparent border-r-transparent border-t-white"></div>
          </div>
        </div>
      )}

      {/* Mobile-friendly expandable details */}
      <div className="mt-2 md:hidden">
        <button
          onClick={() => setShowDetails(!showDetails)}
          className="text-xs text-blue-600 hover:text-blue-800 underline"
        >
          {showDetails ? 'Hide' : 'Show'} fee details
        </button>
        
        {showDetails && (
          <div className="mt-2 p-3 bg-gray-50 rounded-lg text-xs">
            <div className="flex justify-between mb-1">
              <span>Network Fee:</span>
              <span className="font-semibold">{formatXlm(costBreakdown.networkFee)}</span>
            </div>
            <div className="flex justify-between mb-1">
              <span>Deposit:</span>
              <span className="font-semibold">{formatXlm(costBreakdown.bufferAmount)}</span>
            </div>
            <div className="flex justify-between font-bold text-blue-600">
              <span>Total:</span>
              <span>{formatXlm(costBreakdown.networkFee + costBreakdown.bufferAmount)}</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Simple fee display component (lightweight alternative)
 */
export function FeeDisplay({ transactionType }: { transactionType: 'SUBSCRIBE' | 'COLLECT' | 'CANCEL' | 'TOP_UP' }) {
  const fee = calculateNetworkFee(transactionType);
  const description = getFeeDescription(transactionType);
  
  return (
    <div className="inline-flex items-center space-x-2 text-sm text-gray-600 bg-gray-100 px-3 py-1 rounded-full">
      <span>⚡</span>
      <span>{formatXlm(fee)}</span>
      <span className="text-xs text-gray-500">network fee</span>
    </div>
  );
}

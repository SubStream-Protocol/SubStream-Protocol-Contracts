# Script to update the distribute_and_collect function with protocol fee logic

$content = Get-Content "lib.rs"
$modifiedContent = @()
$inFirstFunction = $false
$foundRemaining = $false

for ($i = 0; $i -lt $content.Count; $i++) {
    $line = $content[$i]
    
    # Detect if we're entering the first distribute_and_collect function
    if ($line -match "fn distribute_and_collect\(" -and -not $inFirstFunction) {
        $inFirstFunction = $true
        $modifiedContent += $line
        continue
    }
    
    # Detect if we're leaving the first function
    if ($inFirstFunction -and $line -match "^\s*\}") {
        # Check if this is the end of the first function by looking for the next function
        if ($i + 1 -lt $content.Count -and $content[$i + 1] -match "fn ") {
            $inFirstFunction = $false
        }
        $modifiedContent += $line
        continue
    }
    
    # Replace the specific line in the first function only
    if ($inFirstFunction -and $line -match "let mut remaining = amount_to_payout_tokens;" -and -not $foundRemaining) {
        $foundRemaining = $true
        
        # Add the protocol fee logic
        $modifiedContent += "        let mut remaining = amount_to_payout_tokens;"
        $modifiedContent += ""
        $modifiedContent += "        // Get current protocol fee configuration"
        $modifiedContent += "        let fee_config: ProtocolFeeConfig = env.storage().persistent()"
        $modifiedContent += "            .get(&DataKey::ProtocolFeeConfig)"
        $modifiedContent += "            .unwrap_or(ProtocolFeeConfig {"
        $modifiedContent += "                current_fee_bps: DEFAULT_PROTOCOL_FEE_BPS,"
        $modifiedContent += "                last_updated: 0,"
        $modifiedContent += "                updated_by: env.current_contract_address(),"
        $modifiedContent += "            });"
        $modifiedContent += ""
        $modifiedContent += "        // Calculate protocol fee"
        $modifiedContent += "        let protocol_fee = (amount_to_payout_tokens * fee_config.current_fee_bps as i128) / 10000;"
        $modifiedContent += "        let amount_for_creators = amount_to_payout_tokens - protocol_fee;"
        $modifiedContent += ""
        $modifiedContent += "        // Send protocol fee to treasury (contract admin acts as treasury)"
        $modifiedContent += "        if protocol_fee > 0 {"
        $modifiedContent += "            let treasury: Address = env.storage().persistent()"
        $modifiedContent += "                .get(&DataKey::ContractAdmin)"
        $modifiedContent += "                .expect(`"contract admin not found`");"
        $modifiedContent += "            token_client.transfer(&env.current_contract_address(), &treasury, &protocol_fee);"
        $modifiedContent += "        }"
        continue
    }
    
    $modifiedContent += $line
}

# Write the modified content back
Set-Content -Path "lib_modified.rs" -Value $modifiedContent
Write-Host "Updated distribute_and_collect function with protocol fee logic"

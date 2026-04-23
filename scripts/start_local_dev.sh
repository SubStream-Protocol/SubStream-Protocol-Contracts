#!/bin/bash

# SubStream Protocol - Local Development Startup Script
# This script sets up the Soroban contract, Mercury indexer, and Next.js frontend
# Usage: ./scripts/start_local_dev.sh

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CONTRACTS_DIR="$PROJECT_ROOT/contracts/substream_contracts"
FRONTEND_DIR="$PROJECT_ROOT/frontend"
INDEXER_DIR="$PROJECT_ROOT/indexer"

# Default configuration
NETWORK="${NETWORK:-futurenet}"
RPC_URL="${RPC_URL:-https://rpc-futurenet.stellar.org:443}"
FRIENDBOT_URL="${FRIENDBOT_URL:-https://friendbot-futurenet.stellar.org/api}"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║     SubStream Protocol - Local Development Setup          ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Function to print section headers
print_header() {
    echo -e "\n${GREEN}▶ $1${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# Function to print info messages
print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

# Function to print warnings
print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

# Function to print errors
print_error() {
    echo -e "${RED}✖ $1${NC}"
}

# Check prerequisites
check_prerequisites() {
    print_header "Checking Prerequisites"
    
    local missing_deps=()
    
    # Check for Rust
    if ! command -v rustc &> /dev/null; then
        missing_deps+=("Rust (rustc)")
    else
        print_info "✓ Rust installed: $(rustc --version)"
    fi
    
    # Check for Cargo
    if ! command -v cargo &> /dev/null; then
        missing_deps+=("Cargo")
    else
        print_info "✓ Cargo installed: $(cargo --version)"
    fi
    
    # Check for wasm32-unknown-unknown target
    if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
        missing_deps+=("wasm32-unknown-unknown target")
    else
        print_info "✓ WASM target installed"
    fi
    
    # Check for Stellar CLI (soroban)
    if ! command -v soroban &> /dev/null; then
        missing_deps+=("Stellar CLI (soroban)")
    else
        print_info "✓ Stellar CLI installed: $(soroban --version)"
    fi
    
    # Check for Node.js
    if ! command -v node &> /dev/null; then
        missing_deps+=("Node.js")
    else
        print_info "✓ Node.js installed: $(node --version)"
    fi
    
    # Check for npm
    if ! command -v npm &> /dev/null; then
        missing_deps+=("npm")
    else
        print_info "✓ npm installed: $(npm --version)"
    fi
    
    # Check for jq (JSON processor)
    if ! command -v jq &> /dev/null; then
        missing_deps+=("jq")
    else
        print_info "✓ jq installed"
    fi
    
    if [ ${#missing_deps[@]} -ne 0 ]; then
        print_error "Missing dependencies:"
        for dep in "${missing_deps[@]}"; do
            echo -e "${RED}  • $dep${NC}"
        done
        echo ""
        print_info "Install missing dependencies and run this script again."
        echo ""
        print_info "Installation guide:"
        echo "  Rust: https://www.rust-lang.org/tools/install"
        echo "  Stellar CLI: https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup"
        echo "  Node.js: https://nodejs.org/"
        echo "  jq: https://stedolan.github.io/jq/download/"
        exit 1
    fi
    
    print_info "All prerequisites met!"
}

# Setup Soroban wallet
setup_soroban_wallet() {
    print_header "Setting Up Soroban Wallet"
    
    # Check if .soroban directory exists
    if [ ! -d "$HOME/.soroban" ]; then
        print_info "Creating Soroban config directory..."
        mkdir -p "$HOME/.soroban"
    fi
    
    # Check if network is configured
    if ! soroban config network list 2>/dev/null | grep -q "$NETWORK"; then
        print_info "Configuring $NETWORK network..."
        soroban config network add "$NETWORK" \
            --rpc-url "$RPC_URL" \
            --network-passphrase "Future Network ; February 2022"
    else
        print_info "✓ Network $NETWORK already configured"
    fi
    
    # Generate or load deployer key
    if ! soroban config identity list 2>/dev/null | grep -q "deployer"; then
        print_info "Generating deployer key..."
        soroban config identity generate deployer
    else
        print_info "✓ Deployer key already exists"
    fi
    
    DEPLOYER_ADDRESS=$(soroban config identity address deployer)
    print_info "Deployer address: $DEPLOYER_ADDRESS"
    
    # Fund the deployer account on testnet/futurenet
    print_info "Checking account balance..."
    if ! curl -s "$RPC_URL" -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getLedgerEntries\",\"params\":[[\"AAAAAAAA${DEPLOYER_ADDRESS:1}\" ]]}" | \
        grep -q "entries.*null"; then
        
        print_info "Funding deployer account via Friendbot..."
        FUNDING_RESPONSE=$(curl -s "$FRIENDBOT_URL?addr=$DEPLOYER_ADDRESS")
        
        if echo "$FUNDING_RESPONSE" | grep -q "success"; then
            print_info "✓ Account funded successfully!"
        else
            print_warning "Friendbot funding may have failed. Check manually."
        fi
    else
        print_info "✓ Account already exists on network"
    fi
}

# Build the contract
build_contract() {
    print_header "Building Soroban Contract"
    
    cd "$CONTRACTS_DIR"
    
    print_info "Compiling contract..."
    cargo build --target wasm32-unknown-unknown --release
    
    # Find the compiled WASM file
    WASM_FILE=$(find target/wasm32-unknown-unknown/release -name "*.wasm" 2>/dev/null | head -n 1)
    
    if [ -z "$WASM_FILE" ]; then
        print_error "Failed to find compiled WASM file"
        exit 1
    fi
    
    print_info "✓ Contract compiled: $(basename "$WASM_FILE")"
    
    # Optimize WASM if wasm-opt is available
    if command -v wasm-opt &> /dev/null; then
        print_info "Optimizing WASM file..."
        OPTIMIZED_WASM="${WASM_FILE%.wasm}-optimized.wasm"
        wasm-opt -Oz "$WASM_FILE" -o "$OPTIMIZED_WASM"
        print_info "✓ WASM optimized: $(basename "$OPTIMIZED_WASM")"
        WASM_FILE="$OPTIMIZED_WASM"
    else
        print_warning "wasm-opt not found. Install binaryen for better optimization."
    fi
    
    echo "$WASM_FILE"
}

# Deploy the contract
deploy_contract() {
    print_header "Deploying Contract"
    
    local WASM_FILE="$1"
    
    cd "$CONTRACTS_DIR"
    
    # Upload WASM to network
    print_info "Uploading WASM to network..."
    WASM_HASH=$(soroban contract upload \
        --source deployer \
        --network "$NETWORK" \
        --wasm "$WASM_FILE" \
        2>&1 | grep -o '[A-Z][A-Z0-9]*' | tail -n 1)
    
    if [ -z "$WASM_HASH" ]; then
        print_error "Failed to upload WASM"
        exit 1
    fi
    
    print_info "✓ WASM uploaded: $WASM_HASH"
    
    # Deploy contract instance
    print_info "Deploying contract instance..."
    
    # Constructor arguments (customize based on your contract's __constructor signature)
    CONTRACT_ID=$(soroban contract deploy \
        --source deployer \
        --network "$NETWORK" \
        --wasm-hash "$WASM_HASH" \
        2>&1 | grep -o 'C[A-Z0-9]*' | tail -n 1)
    
    if [ -z "$CONTRACT_ID" ]; then
        print_error "Failed to deploy contract"
        exit 1
    fi
    
    print_info "✓ Contract deployed: $CONTRACT_ID"
    
    # Save contract ID to environment file
    ENV_FILE="$SCRIPT_DIR/.env.local"
    cat > "$ENV_FILE" << EOF
# SubStream Local Development Configuration
# Auto-generated by start_local_dev.sh

CONTRACT_ADDRESS=$CONTRACT_ID
WASM_HASH=$WASM_HASH
DEPLOYER_ADDRESS=$DEPLOYER_ADDRESS
NETWORK=$NETWORK
RPC_URL=$RPC_URL

# Frontend configuration
NEXT_PUBLIC_CONTRACT_ADDRESS=$CONTRACT_ID
NEXT_PUBLIC_NETWORK=$NETWORK
NEXT_PUBLIC_RPC_URL=$RPC_URL

# Indexer configuration
MERCURY_CONTRACT_ID=$CONTRACT_ID
MERCURY_NETWORK=$NETWORK
EOF
    
    print_info "✓ Configuration saved to $ENV_FILE"
}

# Setup Mercury indexer
setup_mercury_indexer() {
    print_header "Setting Up Mercury Indexer"
    
    # Create indexer directory if it doesn't exist
    mkdir -p "$INDEXER_DIR"
    
    # Check if Mercury is installed
    if ! command -v mercury &> /dev/null; then
        print_warning "Mercury indexer not installed"
        print_info "Installing Mercury CLI..."
        
        # Try to install via cargo
        cargo install mercury-indexer 2>/dev/null || {
            print_warning "Failed to install Mercury via cargo"
            print_info "Manual installation required:"
            echo "  Visit: https://github.com/StellarCN/mercury"
            echo "  Or use Docker (see below)"
        }
    fi
    
    # Create Mercury configuration
    MERCURY_CONFIG="$INDEXER_DIR/mercury-config.yaml"
    cat > "$MERCURY_CONFIG" << EOF
# Mercury Indexer Configuration for SubStream Protocol
contract_id: $CONTRACT_ID
network: $NETWORK
rpc_url: $RPC_URL

database:
  type: sqlite
  path: ./indexer_db.sqlite

indexing:
  start_ledger: 0
  poll_interval: 5s
  
server:
  enabled: true
  port: 8080
  cors_enabled: true

logging:
  level: info
  format: json
EOF
    
    print_info "✓ Mercury config created: $MERCURY_CONFIG"
    
    # Create Docker Compose file for easy setup
    DOCKER_COMPOSE_FILE="$INDEXER_DIR/docker-compose.yml"
    cat > "$DOCKER_COMPOSE_FILE" << 'EOF'
version: '3.8'

services:
  mercury:
    image: stellarcn/mercury:latest
    container_name: substream-mercury
    volumes:
      - ./mercury-config.yaml:/app/config.yaml
      - ./indexer_db:/app/data
    ports:
      - "8080:8080"
    environment:
      - MERCURY_CONFIG=/app/config.yaml
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  postgres:
    image: postgres:15-alpine
    container_name: substream-postgres
    volumes:
      - postgres_data:/var/lib/postgresql/data
    environment:
      - POSTGRES_DB=mercury
      - POSTGRES_USER=mercury
      - POSTGRES_PASSWORD=mercury_password
    ports:
      - "5432:5432"
    restart: unless-stopped

volumes:
  postgres_data:
EOF
    
    print_info "✓ Docker Compose file created"
    
    print_info "To start Mercury indexer:"
    echo -e "  ${BLUE}cd $INDEXER_DIR && docker-compose up -d${NC}"
    echo ""
    print_info "Indexer API will be available at: http://localhost:8080"
}

# Setup Next.js frontend
setup_nextjs_frontend() {
    print_header "Setting Up Next.js Frontend"
    
    # Check if frontend directory exists
    if [ ! -d "$FRONTEND_DIR" ]; then
        print_info "Creating Next.js frontend from template..."
        
        # Create frontend directory
        mkdir -p "$FRONTEND_DIR"
        cd "$FRONTEND_DIR"
        
        # Initialize Next.js project
        npx create-next-app@latest . \
            --typescript \
            --tailwind \
            --eslint \
            --app \
            --src-dir \
            --import-alias "@/*" \
            --use-npm \
            --yes
        
        # Install additional dependencies
        print_info "Installing dependencies..."
        npm install @stellar/freighter-api stellar-sdk soroban-client
        
        # Create basic structure
        mkdir -p src/components src/hooks src/lib src/styles
        
        # Create environment file
        cat > .env.local << EOF
NEXT_PUBLIC_CONTRACT_ADDRESS=$CONTRACT_ID
NEXT_PUBLIC_NETWORK=$NETWORK
NEXT_PUBLIC_RPC_URL=$RPC_URL
EOF
        
        # Create README for frontend
        cat > README.md << 'EOF'
# SubStream Frontend

Next.js frontend for SubStream Protocol.

## Getting Started

```bash
npm install
npm run dev
```

## Features

- Connect Freighter wallet
- Browse creator channels
- Subscribe to streams
- Manage subscriptions
- View earnings dashboard

## Tech Stack

- Next.js 14
- TypeScript
- Tailwind CSS
- Stellar SDK
- Freighter Wallet

## Configuration

Update `.env.local` with your contract addresses.
EOF
        
        print_info "✓ Frontend scaffolded"
    else
        print_info "✓ Frontend directory already exists"
        cd "$FRONTEND_DIR"
        
        # Update environment file
        cat > .env.local << EOF
NEXT_PUBLIC_CONTRACT_ADDRESS=$CONTRACT_ID
NEXT_PUBLIC_NETWORK=$NETWORK
NEXT_PUBLIC_RPC_URL=$RPC_URL
EOF
        
        print_info "✓ Frontend environment updated"
    fi
    
    # Create starter component example
    COMPONENTS_DIR="$FRONTEND_DIR/src/components"
    mkdir -p "$COMPONENTS_DIR"
    
    cat > "$COMPONENTS_DIR/SubscribeButton.tsx" << 'EOF'
'use client';

import { useState } from 'react';
import { connect, isConnected } from '@stellar/freighter-api';

interface SubscribeButtonProps {
  creatorAddress: string;
  rate: bigint;
  duration: number;
}

export default function SubscribeButton({ creatorAddress, rate, duration }: SubscribeButtonProps) {
  const [isSubscribing, setIsSubscribing] = useState(false);
  const [txHash, setTxHash] = useState<string | null>(null);

  const handleSubscribe = async () => {
    if (!isConnected()) {
      await connect();
    }

    setIsSubscribing(true);

    try {
      // TODO: Implement actual subscription logic
      alert('Subscribe functionality coming soon!');
    } catch (error) {
      console.error('Subscription failed:', error);
    } finally {
      setIsSubscribing(false);
    }
  };

  return (
    <button
      onClick={handleSubscribe}
      disabled={isSubscribing}
      className="bg-blue-600 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded disabled:opacity-50"
    >
      {isSubscribing ? 'Subscribing...' : 'Subscribe'}
    </button>
  );
}
EOF
    
    print_info "✓ Sample component created"
    
    print_info "To start frontend:"
    echo -e "  ${BLUE}cd $FRONTEND_DIR && npm run dev${NC}"
    echo ""
    print_info "Frontend will be available at: http://localhost:3000"
}

# Print summary
print_summary() {
    print_header "Setup Complete! 🎉"
    
    echo -e "${GREEN}Contract deployed successfully!${NC}"
    echo ""
    echo "Configuration Summary:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "${BLUE}Network:${NC}        $NETWORK"
    echo -e "${BLUE}Contract ID:${NC}    $CONTRACT_ID"
    echo -e "${BLUE}Deployer:${NC}       $DEPLOYER_ADDRESS"
    echo ""
    echo "Next Steps:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "1. Start Mercury Indexer:"
    echo -e "   ${BLUE}cd $INDEXER_DIR && docker-compose up -d${NC}"
    echo ""
    echo "2. Start Frontend:"
    echo -e "   ${BLUE}cd $FRONTEND_DIR && npm run dev${NC}"
    echo ""
    echo "3. Access the app:"
    echo -e "   Frontend:  ${BLUE}http://localhost:3000${NC}"
    echo -e "   Indexer:   ${BLUE}http://localhost:8080${NC}"
    echo ""
    echo "Environment file saved to:"
    echo -e "   ${BLUE}$SCRIPT_DIR/.env.local${NC}"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    print_info "Happy building! 🚀"
}

# Main execution
main() {
    check_prerequisites
    setup_soroban_wallet
    WASM_FILE=$(build_contract)
    deploy_contract "$WASM_FILE"
    setup_mercury_indexer
    setup_nextjs_frontend
    print_summary
}

# Run main function
main "$@"

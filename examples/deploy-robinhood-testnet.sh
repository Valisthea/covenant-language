#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Deploy KairosCoin (Covenant) to Robinhood Chain testnet.
#
#   Chain      : Robinhood Chain testnet (Arbitrum Orbit L2)
#   chainId    : 46630  (0xb626: verified live; the docs page's "0xB616" is wrong)
#   RPC        : https://rpc.testnet.chain.robinhood.com
#   Explorer   : https://explorer.testnet.chain.robinhood.com
#   Faucet     : https://faucet.testnet.chain.robinhood.com   (browser only)
#
# Requires: foundry (cast) + the covenant compiler.
# Usage:
#   export PK=0x<your-testnet-private-key>
#   ./deploy-robinhood-testnet.sh
#
# SAFETY: testnet only. Never point PK at a mainnet-funded key.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

RPC="${RPC:-https://rpc.testnet.chain.robinhood.com}"
EXPLORER="https://explorer.testnet.chain.robinhood.com"
EXPECTED_CHAINID=46630
COVENANT="${COVENANT:-$HOME/Desktop/_Projets_Kairos/covenant-src/target/release/covenant.exe}"
SRC="${SRC:-./coin.cov}"
OUT="${OUT:-./out}"
FEE_BPS="${FEE_BPS:-100}"   # 1.00%

say() { printf '\n\033[1m▸ %s\033[0m\n' "$*"; }
die() { printf '\n\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

# ── 0. Preconditions ────────────────────────────────────────────────────────
command -v cast >/dev/null || die "foundry 'cast' not found, https://getfoundry.sh"
[ -x "$COVENANT" ] || die "covenant compiler not found at: $COVENANT (set \$COVENANT)"
[ -f "$SRC" ]      || die "source not found: $SRC (set \$SRC)"
[ -n "${PK:-}" ]   || die "export PK=0x<testnet-private-key> first"

# ── 1. Chain sanity: fail loudly if we're not where we think ───────────────
say "Checking chain"
CHAINID=$(cast chain-id --rpc-url "$RPC") || die "cannot reach RPC: $RPC"
[ "$CHAINID" = "$EXPECTED_CHAINID" ] \
  || die "wrong chain: got $CHAINID, expected $EXPECTED_CHAINID. Refusing to deploy."
echo "  chainId $CHAINID ✓   block $(cast block-number --rpc-url "$RPC")   gas $(cast gas-price --rpc-url "$RPC") wei"

# ── 2. Funding check: the one step a human must do ─────────────────────────
DEPLOYER=$(cast wallet address --private-key "$PK")
BAL=$(cast balance "$DEPLOYER" --rpc-url "$RPC")
echo "  deployer $DEPLOYER   balance $(cast from-wei "$BAL") ETH"
if [ "$BAL" = "0" ]; then
  die "deployer has 0 testnet ETH.
     Fund it in a browser (the faucet blocks automated requests):
       https://faucet.testnet.chain.robinhood.com
     Address to fund: $DEPLOYER"
fi

# ── 3. Compile ──────────────────────────────────────────────────────────────
say "Compiling $SRC"
"$COVENANT" build "$SRC" --out "$OUT"
BIN="$OUT/KairosCoin.bin"
[ -f "$BIN" ] || die "expected $BIN: did the contract name change?"
echo "  deploy bytecode: $(( $(wc -c < "$BIN") / 2 )) bytes"

# ── 4. Deploy ───────────────────────────────────────────────────────────────
say "Deploying"
# NOTE: foundry 1.6 requires flags BEFORE --create, else it errors with
# "unexpected argument '--rpc-url' found". Verified against anvil.
RECEIPT=$(cast send --rpc-url "$RPC" --private-key "$PK" --json \
            --create "0x$(tr -d '\n\r ' < "$BIN" | sed 's/^0x//')")
ADDR=$(echo "$RECEIPT" | python -c 'import sys,json; print(json.load(sys.stdin)["contractAddress"])')
TXH=$(echo "$RECEIPT" | python -c 'import sys,json; print(json.load(sys.stdin)["transactionHash"])')
[ -n "$ADDR" ] || die "no contractAddress in receipt"
echo "  contract  $ADDR"
echo "  tx        $TXH"

# ── 5. Configure the fee: REQUIRED: fee_recipient defaults to 0x0, so
#        transfer_with_fee would otherwise credit the zero address.
say "Configuring fee (recipient=deployer, ${FEE_BPS}bps)"
cast send "$ADDR" "set_fee(address,uint256)" "$DEPLOYER" "$FEE_BPS" \
  --rpc-url "$RPC" --private-key "$PK" >/dev/null
echo "  set_fee ✓"

# ── 6. Verify on-chain reads ────────────────────────────────────────────────
say "Verifying"
printf '  name()          %s\n' "$(cast call "$ADDR" 'name()(string)'        --rpc-url "$RPC")"
printf '  symbol()        %s\n' "$(cast call "$ADDR" 'symbol()(string)'      --rpc-url "$RPC")"
printf '  decimals()      %s\n' "$(cast call "$ADDR" 'decimals()(uint256)'   --rpc-url "$RPC")"
printf '  totalSupply()   %s\n' "$(cast call "$ADDR" 'totalSupply()(uint256)' --rpc-url "$RPC")"
printf '  balanceOf(dep)  %s\n' "$(cast call "$ADDR" 'balanceOf(address)(uint256)' "$DEPLOYER" --rpc-url "$RPC")"
printf '  fee_rate_bps()  %s\n' "$(cast call "$ADDR" 'fee_rate_bps()(uint256)' --rpc-url "$RPC")"
printf '  burned_total()  %s\n' "$(cast call "$ADDR" 'burned_total()(uint256)' --rpc-url "$RPC")"

say "Done"
cat <<EOF
  Contract : $EXPLORER/address/$ADDR
  Deploy tx: $EXPLORER/tx/$TXH

  Exercise it (milestone evidence):
    cast send $ADDR 'burn(uint256)' 1000000000000000000 --rpc-url $RPC --private-key \$PK
    cast send $ADDR 'transfer_with_fee(address,uint256)' <to> 1000000000000000000 --rpc-url $RPC --private-key \$PK
    cast call $ADDR 'burned_total()(uint256)'  --rpc-url $RPC
    cast call $ADDR 'fees_collected()(uint256)' --rpc-url $RPC

  NOTE: testnet deployment: the token has no monetary value and is not
  tradable for anything real. It is compiler evidence, not a token launch.
  Archive the tx hashes off-chain: a 3-week-old testnet may be reset.
EOF

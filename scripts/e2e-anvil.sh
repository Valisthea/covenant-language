#!/usr/bin/env bash
#
# End-to-end proof that compiler output runs on a real EVM.
#
# Until now the only on-chain check was a direct `cast send` to a deployed
# helper, which proves the helper works, not that a contract the compiler
# generates deploys and behaves. This closes that gap for the portable subset:
# every portable construct is compiled and deployed to a local anvil, and the
# two that synthesize a standard surface, `token` and `nft`, have that surface
# exercised with on-chain assertions.
#
# No key of yours is involved. anvil ships deterministic dev accounts whose
# keys are public and hold nothing; accounts 0 and 1 below are two of them.
#
# Usage:  scripts/e2e-anvil.sh
# Needs:  anvil and cast (Foundry) on PATH, and a built release compiler.
# Exits:  0 if every assertion passes, non-zero on the first failure.

set -euo pipefail

PY="$(command -v python3 || command -v python)"

PORT="${ANVIL_PORT:-8546}"
RPC="http://127.0.0.1:${PORT}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"

# anvil dev accounts 0 and 1. Public, deterministic, unfunded on any real chain.
PK="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
DEPLOYER="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
BOB_PK="0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
BOB="0x70997970C51812dc3A010C7d01b50e0d17dc79C8"

# keccak256("Transfer(address,address,uint256)")
TRANSFER_TOPIC="0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"

COVENANT="${COVENANT:-}"
if [ -z "$COVENANT" ]; then
  for c in "$ROOT/target/release/covenant" "$ROOT/target/release/covenant.exe"; do
    [ -x "$c" ] && COVENANT="$c" && break
  done
fi

say()  { printf '\n== %s ==\n' "$*"; }
pass() { printf '  ok   %s\n' "$*"; }
fail() { printf '  FAIL %s\n' "$*" >&2; exit 1; }
eq()   { [ "$1" = "$2" ] && pass "$3 = $1" || fail "$3: got $1, expected $2"; }

# uint256 return value as a plain integer. `cast call` prints the decimal form
# followed by the hex form in brackets.
num() { cast call "$@" --rpc-url "$RPC" | grep -oE '^[0-9]+'; }

jq_field() { "$PY" -c "import sys,json;d=json.load(sys.stdin);print(d.get('$1',''))"; }

command -v anvil >/dev/null || { echo "anvil not found (https://getfoundry.sh)"; exit 2; }
command -v cast  >/dev/null || { echo "cast not found";  exit 2; }
[ -n "$COVENANT" ] && [ -x "$COVENANT" ] || {
  echo "no release compiler; run: cargo build --release --bin covenant"; exit 2;
}

ANVIL_PID=""
cleanup() {
  [ -n "$ANVIL_PID" ] && kill "$ANVIL_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

say "Starting anvil on port ${PORT}"
anvil --port "$PORT" --silent &
ANVIL_PID=$!
for _ in $(seq 1 30); do
  cast chain-id --rpc-url "$RPC" >/dev/null 2>&1 && break
  sleep 1
done
cast chain-id --rpc-url "$RPC" >/dev/null 2>&1 || fail "anvil did not come up"
pass "anvil up, chainId $(cast chain-id --rpc-url "$RPC")"

# Compile $1 (source) as $2 (contract name) and deploy it. Echoes the address.
build_and_deploy() {
  local source="$1" name="$2" out="$WORK/$2"
  printf '%s\n' "$source" > "$WORK/$name.cov"
  "$COVENANT" build "$WORK/$name.cov" --out "$out" >/dev/null
  local bin_file="$out/$name.bin"
  [ -f "$bin_file" ] || fail "$name: compiler produced no bytecode"
  local bin
  bin="0x$(tr -d '\n\r ' < "$bin_file" | sed 's/^0x//')"
  local addr
  addr="$(cast send --rpc-url "$RPC" --private-key "$PK" --json --create "$bin" \
          | jq_field contractAddress)"
  [ -n "$addr" ] || fail "$name: no contractAddress in receipt"
  [ "$(cast code "$addr" --rpc-url "$RPC")" != "0x" ] || fail "$name: no code at $addr"
  echo "$addr"
}

say "Every portable construct compiles and deploys"
# `record`, `counter` and `module` synthesize no functions by design, so the
# assertion for them is exactly what they promise: the artifact deploys and
# carries runtime code. Claiming more would be the overclaim the capability
# registry exists to prevent.
for pair in \
  "record|record Config { owner: address\n    enabled: bool }" \
  "counter|counter Tally { total: amount }" \
  "module|module Gate { field n: amount\n    action bump() { n += 1 } }"
do
  kind="${pair%%|*}"
  src="$(printf '%b' "${pair#*|}")"
  case "$kind" in
    record) cname=Config ;; counter) cname=Tally ;; module) cname=Gate ;;
  esac
  a="$(build_and_deploy "$src" "$cname")"
  pass "$kind deployed to $a"
done

say "token: deploying the synthesized ERC-20"
ADDR="$(build_and_deploy 'token DemoCoin {
    symbol:   "DEMO"
    name:     "Demo Coin"
    decimals: 18
    supply:   1_000_000 to deployer
}' DemoCoin)"
pass "deployed to $ADDR"

say "token: reading the synthesized surface"
eq "$(cast call "$ADDR" 'name()(string)'   --rpc-url "$RPC")" '"Demo Coin"' "name()"
eq "$(cast call "$ADDR" 'symbol()(string)' --rpc-url "$RPC")" '"DEMO"'      "symbol()"
eq "$(num "$ADDR" 'decimals()(uint8)')"     "18"      "decimals()"
eq "$(num "$ADDR" 'totalSupply()(uint256)')" "1000000" "totalSupply()"
eq "$(num "$ADDR" 'balanceOf(address)(uint256)' "$DEPLOYER")" "1000000" "balanceOf(deployer)"

say "token: transfer moves value and logs it"
tx="$(cast send "$ADDR" 'transfer(address,uint256)' "$BOB" 250000 \
      --rpc-url "$RPC" --private-key "$PK" --json)"
eq "$(echo "$tx" | jq_field status)" "0x1" "transfer status"
dep2="$(num "$ADDR" 'balanceOf(address)(uint256)' "$DEPLOYER")"
bob2="$(num "$ADDR" 'balanceOf(address)(uint256)' "$BOB")"
tot2="$(num "$ADDR" 'totalSupply()(uint256)')"
eq "$dep2" "750000" "balanceOf(deployer) after transfer"
eq "$bob2" "250000" "balanceOf(bob) after transfer"
[ "$(( dep2 + bob2 ))" -eq "$tot2" ] && pass "conservation: $dep2 + $bob2 == $tot2" \
  || fail "conservation broken: $dep2 + $bob2 != $tot2"

# An ERC-20 that moves value without logging Transfer is invisible to every
# indexer, wallet and explorer downstream. The balances alone would not catch it.
echo "$tx" | "$PY" -c "
import sys, json
logs = json.load(sys.stdin)['logs']
if len(logs) != 1:
    sys.exit('expected exactly one log, got %d' % len(logs))
t = logs[0]['topics']
if t[0] != '$TRANSFER_TOPIC':
    sys.exit('topic0 is %s, not Transfer(address,address,uint256)' % t[0])
if len(t) != 3:
    sys.exit('Transfer has %d topics, so from/to are not both indexed' % len(t))
if int(logs[0]['data'], 16) != 250000:
    sys.exit('Transfer value is %d, not 250000' % int(logs[0]['data'], 16))
" || fail "the Transfer event is wrong"
pass "Transfer event: correct topic, from and to indexed, value 250000"

say "token: the allowance path"
cast send "$ADDR" 'approve(address,uint256)' "$BOB" 500 \
     --rpc-url "$RPC" --private-key "$PK" >/dev/null
eq "$(num "$ADDR" 'allowance(address,address)(uint256)' "$DEPLOYER" "$BOB")" "500" "allowance after approve"
cast send "$ADDR" 'transferFrom(address,address,uint256)' "$DEPLOYER" "$BOB" 200 \
     --rpc-url "$RPC" --private-key "$BOB_PK" >/dev/null
eq "$(num "$ADDR" 'allowance(address,address)(uint256)' "$DEPLOYER" "$BOB")" "300" "allowance debited by transferFrom"
eq "$(num "$ADDR" 'balanceOf(address)(uint256)' "$BOB")" "250200" "balanceOf(bob) after transferFrom"

say "nft: deploying the synthesized ERC-721"
NFT="$(build_and_deploy 'nft Badge {
    symbol: "BDG"
    name:   "Badge"
}' Badge)"
cast send "$NFT" 'mint(address,uint256)' "$DEPLOYER" 1 \
     --rpc-url "$RPC" --private-key "$PK" >/dev/null
eq "$(cast call "$NFT" 'ownerOf(uint256)(address)' 1 --rpc-url "$RPC")" "$DEPLOYER" "ownerOf(1) after mint"
eq "$(num "$NFT" 'balanceOf(address)(uint256)' "$DEPLOYER")" "1" "balanceOf(deployer)"
cast send "$NFT" 'transferFrom(address,address,uint256)' "$DEPLOYER" "$BOB" 1 \
     --rpc-url "$RPC" --private-key "$PK" >/dev/null
eq "$(cast call "$NFT" 'ownerOf(uint256)(address)' 1 --rpc-url "$RPC")" "$BOB" "ownerOf(1) after transferFrom"

# Every assertion above passes on a contract that enforces nothing, so the
# suite is only worth as much as the refusals below.
say "Negative controls"

# `--gas-limit` skips client-side estimation, so the transaction is mined and
# leaves a receipt. Without it `cast send` fails locally and ANY failure, a
# dead RPC or a typo in the signature included, would read as a pass. The
# assertion is on the receipt status, which only the EVM can produce.
neg() {
  local label="$1" who="$2"; shift 2
  local rc st
  rc="$(cast send "$@" --rpc-url "$RPC" --private-key "$who" \
        --gas-limit 300000 --json 2>/dev/null || true)"
  st="$(echo "$rc" | jq_field status 2>/dev/null || true)"
  [ -n "$st" ] || fail "$label: no receipt at all, so nothing was proven on-chain"
  eq "$st" "0x0" "$label reverted (receipt status)"
}

before_dep="$(num "$ADDR" 'balanceOf(address)(uint256)' "$DEPLOYER")"
before_bob="$(num "$ADDR" 'balanceOf(address)(uint256)' "$BOB")"

neg "transfer beyond balance" "$PK" "$ADDR" 'transfer(address,uint256)' "$BOB" 999999999
neg "transferFrom beyond allowance" "$BOB_PK" "$ADDR" 'transferFrom(address,address,uint256)' "$DEPLOYER" "$BOB" 100000
neg "transferFrom of an unowned nft" "$PK" "$NFT" 'transferFrom(address,address,uint256)' "$BOB" "$DEPLOYER" 1

# A revert that still moved value would be worse than no check at all.
eq "$(num "$ADDR" 'balanceOf(address)(uint256)' "$DEPLOYER")" "$before_dep" "deployer balance unchanged by the refused calls"
eq "$(num "$ADDR" 'balanceOf(address)(uint256)' "$BOB")"      "$before_bob" "bob balance unchanged by the refused calls"

say "All assertions passed"

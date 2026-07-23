# covenant-verify — source verification for Covenant contracts

Recompile a `.cov` source and prove it produces the bytecode deployed at an
address. Usable as a CLI, as an HTTP microservice, or as a library.

**Why this exists.** Every public explorer verifies contracts by recompiling
Solidity or Vyper. Blockscout ships 8 verification methods; none of them knows
what Covenant is. Until an explorer integrates Covenant, a deployed Covenant
contract's source cannot be checked by anyone but its author — so we ship the
verifier ourselves, and we ship it in a shape an explorer can adopt.

## The guarantee, and its one precondition

Verification reduces to a single claim:

> Compiling **this source** with **this compiler version** produces bytecode
> byte-identical to the runtime code at **this address**.

That is only meaningful if the compiler is **deterministic**. Covenant is —
demonstrated, not assumed:

```
$ covenant build examples/kairos_coin.cov --out /tmp/a
$ covenant build examples/kairos_coin.cov --out /tmp/b
$ sha256sum /tmp/{a,b}/KairosCoin.runtime.bin
12ee49706b8c10d1ed5363c9e13712c2f6113e8067a33bb4406683c99672c5e5  /tmp/a/...
12ee49706b8c10d1ed5363c9e13712c2f6113e8067a33bb4406683c99672c5e5  /tmp/b/...

$ cast code 0x40254d0b63a9AbdB38671dC7DC41f3BaE5B65025 \
      --rpc-url https://rpc.testnet.chain.robinhood.com | sha256sum
12ee49706b8c10d1ed5363c9e13712c2f6113e8067a33bb4406683c99672c5e5
```

Same hash locally, twice, and on chain. (M6 milestone, Covenant 0.9.3.)

We compare **runtime** bytecode, never deploy bytecode: deploy bytecode carries
the constructor and any appended constructor arguments, which legitimately
differ between a local build and a real deployment. Runtime code is what lives
at the address and what users actually trust.

## CLI

```bash
covenant-verify \
  --source   examples/kairos_coin.cov \
  --address  0x40254d0b63a9AbdB38671dC7DC41f3BaE5B65025 \
  --rpc      https://rpc.testnet.chain.robinhood.com \
  --compiler 0.9.3
```

Exit codes: `0` match · `1` mismatch · `2` source failed to compile ·
`3` no contract at address · `4` transport/usage error. Add `--json` for
machine-readable output.

## HTTP service

`POST /api/v1/verify`

```jsonc
{
  "address": "0x40254d0b63a9AbdB38671dC7DC41f3BaE5B65025",
  "chainId": 46630,
  "compilerVersion": "0.9.3",
  "sourceFiles": { "kairos_coin.cov": "token KairosCoin { … }" },
  // optional: skip the RPC round-trip by supplying the code yourself
  "deployedBytecode": "0x6080…"
}
```

```jsonc
{
  "status": "success",              // success | failure | error
  "matchType": "full",              // full | none
  "compilerVersion": "0.9.3",
  "language": "covenant",
  "abi": [ … ],
  "functionSelectors": { "transfer": "0xa9059cbb", … },
  "storageLayout": [ … ],
  "runtimeBytecodeSha256": "12ee4970…c5e5",
  "onchainBytecodeSha256": "12ee4970…c5e5",
  "message": "Byte-for-byte match."
}
```

The response is deliberately shaped after Blockscout's verifier contract
(`status` / `message` / result payload) so integration is a config change on
their side rather than a translation layer.

## For explorer maintainers

Covenant contracts are currently displayed as *unverified source* on every
explorer, which is a worse outcome for your users than for us: the bytecode is
real, the source is real, and nothing lets a reader connect the two.

Three integration paths, cheapest first:

1. **Point at a hosted instance.** Run nothing. Forward Covenant verification
   requests to a `covenant-verify` endpoint and render the response. This is a
   route plus a config entry.
2. **Self-host the service.** One container. It needs outbound access only to
   fetch compiler releases (or ship them baked in for an air-gapped setup).
3. **Vendor the library.** The comparison is ~50 lines: normalize both byte
   strings, hash, compare. The compiler does everything hard.

Multi-version support is the one real requirement: verifying a contract built
with 0.9.0 needs the 0.9.0 compiler, exactly as Etherscan asks which `solc` you
used. Every Covenant artifact records its version in `metadata.json`
(`covenantVersion`), and released compilers are published per version, so the
correct binary is always addressable.

We are not asking anyone to trust us — that is the point of the design. Run the
compiler yourself and compare the hash. The verifier is not an authority; it is
a convenience wrapped around a comparison anyone can repeat.

## Status

Covenant is testnet-only today, its cryptographic primitives are mocked, and it
has had no third-party audit. See [`STATUS.md`](../STATUS.md). None of that
affects this tool's claim, which is narrow and checkable: *these bytes equal
those bytes*.

Reference implementation in TypeScript (runs in a browser, no server needed):
[`covenant-playground/src/lib/verify.ts`](https://github.com/Valisthea/covenant-playground/blob/main/src/lib/verify.ts).
Live: <https://playground.covenant-lang.org/verify>.

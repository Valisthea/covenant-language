/* tslint:disable */
/* eslint-disable */
/**
 * Hand-authored TypeScript declarations for the Covenant WASM bindings.
 *
 * The playground imports this module directly. Keeping the .d.ts hand-
 * written (instead of letting wasm-bindgen generate it) means we can
 * keep the public shape stable across compiler versions — adding a new
 * field to the underlying Rust type is a non-breaking change as long as
 * we don't also change this file.
 *
 * Build artifact layout (after `wasm-pack build`):
 *
 *   covenant_wasm_bindings.js          ← ES module entry (default = init)
 *   covenant_wasm_bindings_bg.wasm     ← the binary
 *   covenant_wasm_bindings.d.ts        ← THIS FILE
 *
 * Runtime contract: the JS module exports a default `init(url?)`
 * function that loads + instantiates the .wasm. After `await init(url)`
 * resolves, all named exports are callable synchronously. The compiler
 * runs on the main thread; expect 50-200ms typical, up to ~1s for large
 * contracts. Move to a Web Worker if you need to keep the UI thread
 * responsive (Sprint 23 territory).
 */

// ─── Diagnostics ──────────────────────────────────────────────────────

export type DiagnosticLevel = 'error' | 'warning' | 'info';

export interface Diagnostic {
  level: DiagnosticLevel;
  /** Stable error code, formatted as `E<NNN>`. */
  code: string;
  message: string;
  /** Optional secondary hint shown by the playground under the message. */
  help: string | null;
  /** 1-indexed line. */
  line: number;
  /** 1-indexed column, in UTF-8 characters (not bytes). */
  column: number;
  end_line: number;
  end_column: number;
  /** Raw byte offsets, half-open. Useful for byte-precise highlights. */
  span_start: number;
  span_end: number;
}

// ─── Artifact pieces ──────────────────────────────────────────────────

export interface FunctionSelector {
  name: string;
  /** 0x-prefixed 4-byte selector, lowercase hex. */
  selector: string;
}

export interface StorageEntry {
  name: string;
  /** 0x-prefixed 32-byte slot, lowercase hex. */
  slot: string;
  offset: number;
  size_bytes: number;
  ty_desc: string;
}

export interface CompilationMetadata {
  covenant_version: string;
  optimizer_config: string;
  evm_version: string;
  erc_versions: Record<string, string>;
  precompile_abi_version: number;
}

export interface SourceMapEntry {
  /** Bytecode offset (PC). */
  pc: number;
  /** 1-indexed line, or 0 if the binding could not resolve. */
  source_line: number;
  /** 1-indexed column, or 0 if the binding could not resolve. */
  source_column: number;
  /** Opcode mnemonic or instruction kind, e.g. `"PUSH1"`, `"CALLDATALOAD"`. */
  instr_kind: string;
}

export interface SourceMap {
  mappings: SourceMapEntry[];
}

export interface Timing {
  /** Wall-clock milliseconds spent compiling. */
  total: number;
}

// ─── Top-level results ────────────────────────────────────────────────

export type CompileTarget = 'evm';

export interface CompileResult {
  ok: boolean;
  target: CompileTarget;
  /** 0x-prefixed deploy bytecode, or null when codegen didn't run. */
  deploy_bytecode: string | null;
  /** 0x-prefixed runtime bytecode, or null. */
  runtime_bytecode: string | null;
  /** ABI as a JSON-encoded string (matches solc output shape). */
  abi: string | null;
  function_selectors: FunctionSelector[];
  storage_layout: StorageEntry[];
  metadata: CompilationMetadata | null;
  source_map: SourceMap | null;
  diagnostics: Diagnostic[];
  timing: Timing;
}

export interface CheckResult {
  diagnostics: Diagnostic[];
  timing: Timing;
}

export interface IrResult {
  ok: boolean;
  /** Pretty-printed IR, or null on failure. */
  ir_text: string | null;
  diagnostics: Diagnostic[];
  timing: Timing;
}

export interface DiagnosticExplanation {
  code: string;
  short: string;
  long: string;
}

// ─── Entry point + named exports ──────────────────────────────────────

/**
 * Initialise the WASM module. Call once before any of the named
 * exports below.
 *
 * @param input - URL or fetch input that resolves to the .wasm binary.
 *                Pass `new URL('covenant_wasm_bindings_bg.wasm', import.meta.url)`
 *                for the typical static-asset case.
 */
export default function init(
  input?: RequestInfo | URL,
): Promise<unknown>;

/** Returns the compiler version string, e.g. `"0.8.2"`. */
export function version(): string;

/**
 * Compile a Covenant source string targeting EVM bytecode.
 *
 * Always returns a result; failures are conveyed via `result.ok` and
 * `result.diagnostics`. Panics inside the compiler surface as JS
 * exceptions thanks to the panic hook.
 */
export function compile_to_evm(source: string): CompileResult;

/**
 * Run only the frontend stages (lex → parse → resolve → typecheck →
 * privacy). Cheap enough for keystroke-rate calls — used by Monaco's
 * live diagnostics.
 */
export function check(source: string): CheckResult;

/** Compile through IR construction and return it as printable text. */
export function compile_to_ir_text(source: string): IrResult;

/**
 * Diagnostic-code → prose-explanation table. Reserved surface;
 * currently returns `[]` until the compiler ships long-form
 * explanations.
 */
export function diagnostic_explanations(): DiagnosticExplanation[];

// ════════════════════════════════════════════════════════════════════
// Sprint 23 — MockChain bindings.
// ════════════════════════════════════════════════════════════════════
//
// In-tab EVM. Every `chain_*` function calls into a singleton
// `Chain` living in WASM memory. There is one chain per page load —
// reloading the tab resets it. The chain holds 5 prefunded accounts
// (1000 ETH each), an event log, a clock, and a block counter.
//
// Argument convention: every state-mutating entry point that needs
// "complex args" takes a single JSON string. Build it via
// `JSON.stringify({ ... })`. Returns are always JS objects (not
// JSON strings) — `serde-wasm-bindgen::to_value` builds them
// natively, no second `JSON.parse` needed.
//
// Hex convention: every byte field is a `"0x"`-prefixed lowercase
// hex string. U256 values are minimal-length hex (no leading zeros
// beyond the single `"0x0"` for zero).

/** Snapshot of the chain state — fits the playground's status bar. */
export interface ChainState {
  block_number: number;
  timestamp: number;          // unix seconds
  contracts_count: number;
  accounts_count: number;
  tx_count: number;
  chain_id: number;           // 31337 by convention
}

/** One of the 5 prefunded test accounts. */
export interface ChainAccount {
  address: string;            // "0x" + 40 hex chars
  balance: string;            // "0x" + minimal hex
  nonce: number;
  label: string;              // e.g. "Account #1"
}

/** Slim view of a deployed contract (omits runtime bytecode + storage). */
export interface ChainContract {
  address: string;
  deployer: string;
  deployed_at_block: number;
  deployed_at_timestamp: number;
  label: string | null;
  code_hash: string;          // "0x" + 64 hex chars
  runtime_bytecode_size: number;
}

/** One log entry emitted by a LOG{0..4} opcode. */
export interface ChainLogEvent {
  address: string;
  topics: string[];           // each: "0x" + 64 hex chars
  data: string;               // "0x" + 2N hex chars
}

/** Discriminated union: deploy vs call vs static_call. */
export type ChainTxKind =
  | { type: 'deploy' }
  | { type: 'call'; selector: string; calldata: string }
  | { type: 'static_call'; selector: string; calldata: string };

/** Discriminated union: success / reverted / aborted. */
export type ChainTxStatus =
  | { status: 'success' }
  | { status: 'reverted'; reason: string | null }
  | { status: 'aborted'; reason: string };

/** Full transaction receipt — what every chain_deploy / chain_call returns. */
export interface ChainTxReceipt {
  hash: string;               // "0x" + 64 hex chars
  block_number: number;
  timestamp: number;
  from: string;
  to: string | null;          // null when a deploy reverted
  kind: ChainTxKind;
  gas_used: number;           // fixed estimate (10000) — see chain.rs
  status: ChainTxStatus;
  return_data: string;        // "0x" + 2N hex chars
  logs: ChainLogEvent[];
}

// ─── chain_* exports ────────────────────────────────────────────────

/** Reset the chain to genesis (5 prefunded accounts, block 1). */
export function chain_init(): void;

/** Alias for `chain_init`. Lets the JS call site read as `chain_reset()`. */
export function chain_reset(): void;

/**
 * Deploy a contract.
 *
 * @param args_json - JSON-stringified `{ deployer, bytecode_hex,
 *                    constructor_args_hex? }`. Hex fields are `"0x"`-prefixed.
 */
export function chain_deploy(args_json: string): ChainTxReceipt;

/**
 * State-mutating call.
 *
 * @param args_json - JSON-stringified `{ from, to, calldata_hex,
 *                    value_wei_hex? }`.
 */
export function chain_call(args_json: string): ChainTxReceipt;

/**
 * Read-only call. Storage changes are dropped, no log entry appended
 * to the chain's tx_log.
 *
 * @param args_json - JSON-stringified `{ from, to, calldata_hex }`.
 */
export function chain_static_call(args_json: string): ChainTxReceipt;

/** Move the chain clock forward by `seconds`. */
export function chain_advance_time(seconds: number): void;

/**
 * Mine `count` blocks. Each block also bumps the clock by 12s
 * (post-Merge cadence).
 */
export function chain_mine_blocks(count: number): void;

/** Snapshot of the chain — block number, timestamp, counters. */
export function chain_get_state(): ChainState;

/** All 5 prefunded accounts with their current balances. */
export function chain_get_accounts(): ChainAccount[];

/** All deployed contracts (slim view — no bytecode payload). */
export function chain_get_contracts(): ChainContract[];

/** Ordered transaction history since chain init. */
export function chain_get_tx_log(): ChainTxReceipt[];

/**
 * Read a single storage slot.
 *
 * @param address_hex - Contract address, `"0x"`-prefixed.
 * @param slot_hex    - Slot key as `"0x"`-prefixed hex (max 32 bytes).
 * @returns The slot value as a `"0x"`-prefixed minimal hex string.
 */
export function chain_get_storage(address_hex: string, slot_hex: string): string;

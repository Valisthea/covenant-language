//! Aster Chain precompile address table.

use std::collections::HashMap;

use crate::{diagnostics::AsterDiagnostic, lowering::AsterIr};

/// Reserved precompile addresses for Aster-native cryptographic primitives.
pub mod addresses {
    // FHE operations (0x0300-0x0308)
    pub const FHE_ADD: u64 = 0x0300;
    pub const FHE_MUL: u64 = 0x0301;
    pub const FHE_CMP_EQ: u64 = 0x0302;
    pub const FHE_CMP_LT: u64 = 0x0303;
    pub const FHE_CMP_NE: u64 = 0x0304;
    pub const FHE_CMP_LE: u64 = 0x0305;
    pub const FHE_ENCRYPT_FRESH: u64 = 0x0306;
    pub const FHE_DECRYPT: u64 = 0x0307;
    pub const FHE_BOOTSTRAP: u64 = 0x0308;

    // Post-quantum primitives (0x0400-0x0401)
    pub const DILITHIUM_VERIFY: u64 = 0x0400;
    pub const PQ_RAND: u64 = 0x0401;

    // ZK verification (0x0500-0x0501)
    pub const ZK_VERIFY_NOVA: u64 = 0x0500;
    pub const ZK_VERIFY_HALO2: u64 = 0x0501;
}

#[derive(Debug, Default)]
pub struct PrecompileMap {
    pub addresses: HashMap<String, u64>,
}

pub fn resolve(
    _aster_ir: &AsterIr<'_>,
    _warnings: &mut Vec<AsterDiagnostic>,
) -> Result<PrecompileMap, String> {
    let mut map = PrecompileMap::default();
    map.addresses.insert("fhe_add".into(), addresses::FHE_ADD);
    map.addresses.insert("fhe_mul".into(), addresses::FHE_MUL);
    map.addresses
        .insert("fhe_cmp_eq".into(), addresses::FHE_CMP_EQ);
    map.addresses
        .insert("fhe_cmp_lt".into(), addresses::FHE_CMP_LT);
    map.addresses
        .insert("fhe_cmp_ne".into(), addresses::FHE_CMP_NE);
    map.addresses
        .insert("fhe_cmp_le".into(), addresses::FHE_CMP_LE);
    map.addresses
        .insert("fhe_encrypt_fresh".into(), addresses::FHE_ENCRYPT_FRESH);
    map.addresses
        .insert("fhe_decrypt".into(), addresses::FHE_DECRYPT);
    map.addresses
        .insert("fhe_bootstrap".into(), addresses::FHE_BOOTSTRAP);
    map.addresses
        .insert("dilithium_verify".into(), addresses::DILITHIUM_VERIFY);
    map.addresses.insert("pq_rand".into(), addresses::PQ_RAND);
    map.addresses
        .insert("zk_verify_nova".into(), addresses::ZK_VERIFY_NOVA);
    map.addresses
        .insert("zk_verify_halo2".into(), addresses::ZK_VERIFY_HALO2);
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::addresses::*;

    #[test]
    fn all_addresses_unique() {
        let ids = [
            FHE_ADD,
            FHE_MUL,
            FHE_CMP_EQ,
            FHE_CMP_LT,
            FHE_CMP_NE,
            FHE_CMP_LE,
            FHE_ENCRYPT_FRESH,
            FHE_DECRYPT,
            FHE_BOOTSTRAP,
            DILITHIUM_VERIFY,
            PQ_RAND,
            ZK_VERIFY_NOVA,
            ZK_VERIFY_HALO2,
        ];
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            assert!(
                seen.insert(id),
                "Duplicate precompile address: 0x{:04x}",
                id
            );
        }
    }
}

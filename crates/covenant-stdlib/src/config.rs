//! Stdlib lowering configuration.

#[derive(Debug, Clone, Copy)]
pub struct StdlibConfig {
    pub synthesize_erc20: bool,
    pub synthesize_erc8227: bool,
    /// V0.9 Sprint 35.b: ERC-721 (NFT) auto-synthesis for `nft` constructs.
    pub synthesize_erc721: bool,
    /// V0.9 Sprint 35.b: ERC-8231 (post-quantum key registry) auto-synthesis
    /// for `registry` constructs.
    pub synthesize_erc8231: bool,
    pub synthesize_ballot_queries: bool,
    pub synthesize_ceremony_queries: bool,
    pub synthesize_registry_queries: bool,
    pub synthesize_vault_queries: bool,
    pub auto_inject_fields: bool,
    pub strict_conflict_detection: bool,
}

impl Default for StdlibConfig {
    fn default() -> Self {
        Self {
            synthesize_erc20: true,
            synthesize_erc8227: true,
            synthesize_erc721: true,
            synthesize_erc8231: true,
            synthesize_ballot_queries: false,
            synthesize_ceremony_queries: true,
            synthesize_registry_queries: false,
            synthesize_vault_queries: false,
            auto_inject_fields: true,
            strict_conflict_detection: true,
        }
    }
}

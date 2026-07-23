//! Optimizer configuration knobs.

#[derive(Debug, Clone, Copy)]
pub struct OptimizerConfig {
    pub enable_precompute: bool,
    pub enable_constant_folding: bool,
    pub enable_dce: bool,
    pub enable_cse: bool,
    pub enable_sload_coalescing: bool,
    pub enable_trivial_fhe_folding: bool,

    pub max_iterations: u32,
    pub validate_after_each_pass: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enable_precompute: true,
            enable_constant_folding: true,
            enable_dce: true,
            enable_cse: true,
            enable_sload_coalescing: true,
            enable_trivial_fhe_folding: true,
            max_iterations: 10,
            validate_after_each_pass: cfg!(debug_assertions),
        }
    }
}

impl OptimizerConfig {
    /// All passes disabled — useful for testing that disabling flags preserves IR.
    pub fn none() -> Self {
        Self {
            enable_precompute: false,
            enable_constant_folding: false,
            enable_dce: false,
            enable_cse: false,
            enable_sload_coalescing: false,
            enable_trivial_fhe_folding: false,
            max_iterations: 10,
            validate_after_each_pass: false,
        }
    }
}

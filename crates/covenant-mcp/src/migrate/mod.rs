pub mod patterns;

pub struct MigrateResult {
    pub output: String,
    pub construct: String,
    pub applied: Vec<String>,
    pub todos: Vec<String>,
}

/// Apply all 11 Solidity→Covenant transformations and classify the best construct.
pub fn apply(source: &str) -> MigrateResult {
    let construct = patterns::classify_construct(source);
    let (output, applied, todos) = patterns::transform(source, &construct);
    MigrateResult {
        output,
        construct,
        applied,
        todos,
    }
}

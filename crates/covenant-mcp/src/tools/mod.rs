pub mod check_syntax;
pub mod compile;
pub mod explain;
pub mod lint;
pub mod list_constructs;
pub mod migrate;
pub mod scaffold;

use rmcp::model::Tool;

/// Returns the definition (name + description + JSON Schema) for every tool.
pub fn all_definitions() -> Vec<Tool> {
    vec![
        compile::definition(),
        check_syntax::definition(),
        lint::definition(),
        scaffold::definition(),
        migrate::definition(),
        explain::definition(),
        list_constructs::definition(),
    ]
}

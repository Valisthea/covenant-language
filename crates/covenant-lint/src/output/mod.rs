pub mod human;
pub mod json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "human" | "text" => Some(Self::Human),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

pub mod diagram;
pub mod report;

pub use diagram::{to_d2, to_mermaid, DiagramOptions, DiagramScope};
pub use report::to_markdown;

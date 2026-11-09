//! Static HTML view and portfolio roll-up.
mod html;
mod rollup;
mod spec_page;
mod status;

pub use rollup::{collect_rollup, export_rollup_json, render_rollup, ProjectState, RollupRow};
pub use spec_page::{render_spec, render_spec_with_reports};
pub use status::load_status;

pub(crate) use html::write_text;
pub(crate) use spec_page::render_dir;

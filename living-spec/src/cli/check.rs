use crate::core::{annotations::Annotations, spec::Spec};
use std::{error::Error, fs, path::Path};

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let [dir] = args else {
        return Err(super::Exit::Usage("usage: living-spec check <spec-dir>".into()).into());
    };
    let dir = Path::new(dir);
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md"))?)?;
    let annotations = Annotations::load(dir.join("annotations.json"))?;
    let orphans = annotations.orphans(&spec);
    println!("{} requirement lines", spec.lines.len());
    println!(
        "orphans: {}",
        if orphans.is_empty() {
            "none".to_owned()
        } else {
            orphans.join(", ")
        }
    );
    if orphans.is_empty() {
        Ok(())
    } else {
        Err("orphan annotations found".into())
    }
}

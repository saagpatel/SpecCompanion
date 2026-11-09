mod audit;
mod check;
mod link;
use std::error::Error;
pub mod boundary;
pub mod quantifier;

#[derive(Debug)]
pub enum Exit {
    InvalidInput(Box<dyn Error>),
    Challenged,
    Broken,
    BoundaryCoverage,
    DependencyCycles,
    Usage(String),
}
impl Exit {
    pub fn code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) | Self::Challenged => 1,
            Self::Broken => 2,
            Self::BoundaryCoverage => 3,
            Self::DependencyCycles => 4,
            Self::Usage(_) => 64,
        }
    }
}
impl std::fmt::Display for Exit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(error) => write!(f, "invalid input: {error}"),
            Self::Challenged => f.write_str("audit challenged one or more links"),
            Self::Broken => f.write_str("broken requirement lines found"),
            Self::BoundaryCoverage => {
                f.write_str("boundary coverage has untested or failing cases")
            }
            Self::DependencyCycles => f.write_str("dependency cycles found"),
            Self::Usage(message) => f.write_str(message),
        }
    }
}
impl Error for Exit {}
impl From<Box<dyn Error>> for Exit {
    fn from(error: Box<dyn Error>) -> Self {
        match error.downcast::<Self>() {
            Ok(exit) => *exit,
            Err(error) => Self::InvalidInput(error),
        }
    }
}
pub fn run(args: &[String]) -> Result<(), Exit> {
    let registry = std::env::var("LIVING_SPEC_REGISTRY").ok();
    run_with_registry_env(args, registry.as_deref())
}
pub fn run_with_registry_env(args: &[String], _registry_env: Option<&str>) -> Result<(), Exit> {
    let result = match args {
        [command, dir] if command == "check" => check::run(std::slice::from_ref(dir)),
        [command, rest @ ..] if command == "link" => link::run(rest),
        [command, rest @ ..] if command == "audit" => audit::run(rest),
        [command, rest @ ..] if command == "boundary" => boundary::run(rest),
        [command, rest @ ..] if command == "quantifier" => Ok(quantifier::run(rest)?),
        _ => Err(Exit::Usage(
            "usage: living-spec check|score|rescore|render|rollup|boundary|deps|quantifier|link|audit <args>".into(),
        )
        .into()),
    };
    result.map_err(Exit::from)
}

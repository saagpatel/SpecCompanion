use super::Exit;
use crate::pipeline::score::{self, CommandExit};
use std::error::Error;

pub use crate::pipeline::score::load_fingerprints;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    score::run(args).map_err(|error| match error.downcast::<CommandExit>() {
        Ok(exit) => match *exit {
            CommandExit::Usage(message) => Box::new(Exit::Usage(message)) as Box<dyn Error>,
            CommandExit::Broken => Box::new(Exit::Broken) as Box<dyn Error>,
        },
        Err(error) => error,
    })
}

use super::badge::Badge;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transition {
    pub at: String,
    pub commit: String,
    pub requirement: String,
    pub from: Badge,
    pub to: Badge,
    pub test_id: String,
    pub note: String,
}
#[derive(Debug, Clone)]
pub struct History {
    path: PathBuf,
}
impl History {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }
    pub fn append(&self, transition: &Transition) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(transition)?;
        bytes.push(b'\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(&bytes)
    }
    pub fn read(&self) -> io::Result<Vec<Transition>> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        BufReader::new(file)
            .lines()
            .map(|line| serde_json::from_str(&line?).map_err(io::Error::from))
            .collect()
    }
    pub fn history_for(&self, id: &str) -> io::Result<Vec<Transition>> {
        Ok(self
            .read()?
            .into_iter()
            .filter(|t| t.requirement == id)
            .collect())
    }
    pub fn last_transition(&self, id: &str) -> io::Result<Option<Transition>> {
        Ok(self.history_for(id)?.pop())
    }
}

use super::spec::Spec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::Path,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotations {
    pub schema: String,
    pub requirements: BTreeMap<String, RequirementAnnotations>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RequirementAnnotations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<Dependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounds: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantifier: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Link {
    pub test_id: String,
    pub fingerprint: String,
    #[serde(default)]
    pub rename_hint: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
    pub explanation: String,
    pub proposed_by: String,
    pub audit: Audit,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Audit {
    pub verdict: AuditVerdict,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
    pub notes: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Audit {
    /// Overrides are explicit authored decisions, separate from the auditor's verdict.
    pub fn human_override_reason(&self) -> Option<&str> {
        let value = self.extra.get("override")?;
        if value.get("by")?.as_str()? != "human" {
            return None;
        }
        value
            .get("reason")?
            .as_str()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn is_accepted(&self) -> bool {
        self.verdict == AuditVerdict::Accepted || self.human_override_reason().is_some()
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditVerdict {
    Accepted,
    Challenged,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependency {
    pub id: String,
    pub evidence: String,
}
impl Annotations {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let annotations: Self = serde_json::from_reader(fs::File::open(path)?)?;
        for annotation in annotations.requirements.values() {
            for link in &annotation.links {
                if link.audit.extra.contains_key("override")
                    && link.audit.human_override_reason().is_none()
                {
                    return Err("audit.override requires by: human and a non-empty reason".into());
                }
            }
        }
        Ok(annotations)
    }
    /// Write deterministic pretty JSON with a final newline.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        fs::write(path, json)
    }
    pub fn orphans(&self, spec: &Spec) -> Vec<String> {
        let ids: BTreeSet<&str> = spec.lines.iter().map(|line| line.id.as_str()).collect();
        self.requirements
            .keys()
            .filter(|id| !ids.contains(id.as_str()))
            .cloned()
            .collect()
    }
}

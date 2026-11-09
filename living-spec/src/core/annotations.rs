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
#[serde(
    from = "RequirementAnnotationsWire",
    into = "RequirementAnnotationsWire"
)]
pub struct RequirementAnnotations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<Dependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounds: Vec<Value>,
    /// Preserve absent bounds versus an explicitly authored empty override.
    #[serde(skip)]
    pub bounds_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantifier: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct RequirementAnnotationsWire {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<Link>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<Dependency>,
    #[serde(
        default,
        deserialize_with = "present_bounds",
        skip_serializing_if = "Option::is_none"
    )]
    bounds: Option<Vec<Value>>,
    #[serde(
        default,
        deserialize_with = "present_quantifier",
        skip_serializing_if = "Option::is_none"
    )]
    quantifier: Option<Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
fn present_bounds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<Value>>, D::Error> {
    Vec::deserialize(d).map(Some)
}
fn present_quantifier<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    let value = Value::deserialize(d)?;
    if !value.is_object() {
        return Err(serde::de::Error::custom("quantifier must be an object"));
    }
    Ok(Some(value))
}
impl From<RequirementAnnotationsWire> for RequirementAnnotations {
    fn from(wire: RequirementAnnotationsWire) -> Self {
        Self {
            bounds_present: wire.bounds.is_some(),
            bounds: wire.bounds.unwrap_or_default(),
            links: wire.links,
            depends_on: wire.depends_on,
            quantifier: wire.quantifier,
            extra: wire.extra,
        }
    }
}
impl From<RequirementAnnotations> for RequirementAnnotationsWire {
    fn from(annotation: RequirementAnnotations) -> Self {
        Self {
            bounds: (annotation.bounds_present || !annotation.bounds.is_empty())
                .then_some(annotation.bounds),
            links: annotation.links,
            depends_on: annotation.depends_on,
            quantifier: annotation.quantifier,
            extra: annotation.extra,
        }
    }
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
impl Link {
    /// A human decision is live only for the exact evidence binding reviewed.
    pub fn human_override_reason(&self) -> Option<&str> {
        let value = self.audit.extra.get("override")?;
        if value.get("test_id")?.as_str()? != self.test_id
            || value.get("fingerprint")?.as_str()? != self.fingerprint
            || value.get("revision")?.as_str()? != self.revision
            || self.audit.revision != self.revision
            || self.revision.is_empty()
        {
            return None;
        }
        self.audit.authored_override_reason()
    }

    pub fn is_accepted(&self) -> bool {
        self.audit.verdict == AuditVerdict::Accepted || self.human_override_reason().is_some()
    }

    pub fn retire_stale_override(&mut self) {
        if self.human_override_reason().is_none() {
            self.invalidate_override();
        }
    }

    /// Preserve the authored decision as diagnostics, never as acceptance.
    pub fn invalidate_override(&mut self) -> Option<Value> {
        let previous = self.audit.extra.remove("override")?;
        let event = serde_json::json!({
            "override": previous,
            "test_id": self.test_id,
            "fingerprint": self.fingerprint,
            "revision": self.revision,
            "audit_revision": self.audit.revision,
        });
        let history = self
            .audit
            .extra
            .entry("invalidated_overrides".into())
            .or_insert_with(|| serde_json::json!([]));
        if !history.is_array() {
            *history = serde_json::json!([history.clone()]);
        }
        history.as_array_mut().unwrap().push(event);
        Some(previous)
    }
}
impl Audit {
    // Validate authorship only; acceptance also requires the containing Link binding.
    fn authored_override_reason(&self) -> Option<&str> {
        let value = self.extra.get("override")?;
        if value.get("by")?.as_str()? != "human" {
            return None;
        }
        value
            .get("reason")?
            .as_str()
            .filter(|s| !s.trim().is_empty())
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Annotations {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let annotations: Self = serde_json::from_reader(fs::File::open(path)?)?;
        if annotations.schema != "living-spec.annotations/v1" {
            return Err("unsupported annotations schema".into());
        }
        for (id, annotation) in &annotations.requirements {
            validate_typed_sections(id, annotation)?;
            for link in &annotation.links {
                if link.audit.extra.contains_key("override")
                    && link.audit.authored_override_reason().is_none()
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

// Keep the original JSON for writes, but validate typed sections on every load.
fn validate_typed_sections(
    id: &str,
    annotation: &RequirementAnnotations,
) -> Result<(), Box<dyn std::error::Error>> {
    use serde::de::DeserializeOwned;
    fn check<T: DeserializeOwned>(
        value: Value,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fn find_field(value: &Value, key: &str, path: &str, found: &mut Vec<String>) {
            match value {
                Value::Object(fields) => {
                    for (name, child) in fields {
                        let next = format!("{path}/{}", name.replace('~', "~0").replace('/', "~1"));
                        if name == key {
                            found.push(next.clone());
                        }
                        find_field(child, key, &next, found);
                    }
                }
                Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        find_field(child, key, &format!("{path}/{index}"), found);
                    }
                }
                _ => {}
            }
        }
        serde_path_to_error::deserialize::<_, T>(value.clone())
            .map(|_| ())
            .map_err(|error| {
                let message = error.inner().to_string();
                // Internally tagged enums buffer their contents, losing nested serde paths.
                // Locate the reported field in the original JSON and show all matches.
                let key = message
                    .strip_prefix("unknown field `")
                    .and_then(|s| s.split('`').next())
                    .or_else(|| {
                        message
                            .strip_prefix("invalid value: string \"")
                            .and_then(|s| s.split('"').next())
                    });
                let mut locations = Vec::new();
                if let Some(key) = key {
                    find_field(&value, key, path, &mut locations);
                }
                if locations.is_empty() {
                    format!("{path}.{}: {message}", error.path()).into()
                } else {
                    format!("{}: {message}", locations.join(", ")).into()
                }
            })
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Cases {
        #[allow(dead_code)]
        quantity: String,
        #[allow(dead_code)]
        cases: Vec<super::boundary::Example>,
    }
    #[derive(Deserialize)]
    #[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
    enum Source {
        Authored,
        Inferred {
            #[allow(dead_code)]
            revision: String,
        },
    }
    let base = format!("requirements[{id:?}]");
    for (index, bound) in annotation.bounds.iter().enumerate() {
        // Serde's internally tagged unit variants ignore extra keys even with
        // deny_unknown_fields, so check the operation object explicitly.
        if let Some(op) = bound.pointer("/kind/op").and_then(Value::as_object) {
            let allowed: &[&str] = if op.get("type").and_then(Value::as_str) == Some("range") {
                &["type", "lo", "hi", "inclusive_lo", "inclusive_hi"]
            } else {
                &["type"]
            };
            if let Some(key) = op.keys().find(|key| !allowed.contains(&key.as_str())) {
                return Err(
                    format!("{base}.bounds[{index}]/kind/op/{key}: unknown field `{key}`").into(),
                );
            }
        }
        check::<super::boundary::model::Bound>(bound.clone(), &format!("{base}.bounds[{index}]"))?;
    }
    if let Some(value) = &annotation.quantifier {
        let path = format!("{base}.quantifier");
        if value.get("sampling").is_some_and(Value::is_null) {
            return Err(format!("{path}.sampling: expected an object").into());
        }
        check::<super::quantifier::Quantifier>(value.clone(), &path)?;
        let quantifier: super::quantifier::Quantifier = serde_json::from_value(value.clone())?;
        validate_quantifier_schema(&quantifier, &path)?;
    }
    for (key, value) in &annotation.extra {
        let path = format!("{base}.{key}");
        match key.as_str() {
            "boundary_cases" => check::<Vec<Cases>>(value.clone(), &path)?,
            "boundary_links" => {
                check::<Vec<super::boundary::coverage::CaseLink>>(value.clone(), &path)?
            }
            "bounds_source" => {
                if let Some(source) = value.as_object() {
                    let allowed: &[&str] =
                        if source.get("origin").and_then(Value::as_str) == Some("inferred") {
                            &["origin", "revision"]
                        } else {
                            &["origin"]
                        };
                    if let Some(key) = source.keys().find(|key| !allowed.contains(&key.as_str())) {
                        return Err(format!("{path}/{key}: unknown field `{key}`").into());
                    }
                }
                check::<Source>(value.clone(), &path)?
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_quantifier_schema(
    quantifier: &super::quantifier::Quantifier,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use super::quantifier::{Domain, Kind};

    const SUBJECTS: &[&str] = &[
        "abs_i8",
        "abs_wrapping_i8",
        "clamp",
        "clamp_buggy",
        "saturating_add",
        "percent_of",
        "percent_of_buggy",
        "utf8_roundtrip",
        "utf8_roundtrip_buggy",
        "sort_vec",
        "sort_vec_buggy",
        "dedup_vec",
        "bytes_roundtrip",
    ];
    if !SUBJECTS.contains(&quantifier.subject.as_str()) {
        return Err(format!("{path}.subject: unsupported fixture subject").into());
    }
    if let Some(sampling) = &quantifier.sampling {
        if !(1..=10_000).contains(&sampling.samples) {
            return Err(format!("{path}.sampling.samples: must be 1..10000").into());
        }
    }

    fn finite(domain: &Domain) -> bool {
        match domain {
            Domain::Unbounded { .. } => false,
            Domain::Product { parts } => parts.iter().all(finite),
            _ => true,
        }
    }
    let finite_domain = finite(&quantifier.domain);
    match (
        quantifier.kind,
        finite_domain,
        quantifier.sampling.is_some(),
    ) {
        (Kind::FiniteUniversal, false, _) => {
            return Err(format!("{path}.domain: finite-universal requires a finite domain").into())
        }
        (Kind::FiniteUniversal, true, true) => {
            return Err(format!("{path}.sampling: finite-universal does not allow sampling").into())
        }
        (Kind::InfiniteUniversal, true, _) => {
            return Err(
                format!("{path}.domain: infinite-universal requires an unbounded domain").into(),
            )
        }
        (Kind::InfiniteUniversal, false, false) => {
            return Err(format!("{path}.sampling: infinite-universal requires sampling").into())
        }
        (Kind::Existential, true, true) => {
            return Err(
                format!("{path}.sampling: finite existential does not allow sampling").into(),
            )
        }
        (Kind::Existential, false, false) => {
            return Err(format!("{path}.sampling: infinite existential requires sampling").into())
        }
        _ => {}
    }
    Ok(())
}

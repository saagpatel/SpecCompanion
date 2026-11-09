pub mod finite;
pub mod sampled;
use super::{
    annotations::{Kind, Value, WORK_CAP},
    catalog::{ClaimChecker, FixtureCatalog},
    predicate::Budget,
    Claim, Coverage, Observation, Outcome, Report, Result,
};
use std::collections::BTreeSet;
pub(crate) fn execute(
    claim: &Claim,
    inputs: impl Iterator<Item = Value>,
    mut coverage: Coverage,
) -> Result<Report> {
    let mut budget = Budget::new(WORK_CAP);
    let mut observations = Vec::new();
    let mut distinct = BTreeSet::new();
    for input in inputs {
        budget.spend(input.cost() * 8 + 1)?;
        let output = FixtureCatalog.observe(&claim.quantifier.subject, &input)?;
        budget.spend(output.cost())?;
        let satisfied = claim
            .quantifier
            .predicate
            .evaluate(&input, &output, &mut budget)?;
        distinct.insert(input.clone());
        observations.push(Observation {
            input,
            output,
            satisfied,
        });
    }
    coverage.tested_cases = observations.len() as u64;
    coverage.distinct_tested = distinct.len() as u64;
    coverage.untested_remainder = match coverage.finite_total {
        Some(n) => format!(
            "{} cases in the declared finite domain untested",
            n - coverage.distinct_tested
        ),
        None => format!(
            "unbounded; {} distinct values sampled; all other values untested",
            coverage.distinct_tested
        ),
    };
    let witness = if claim.quantifier.kind == Kind::Existential {
        observations.iter().find(|o| o.satisfied).cloned()
    } else {
        None
    };
    let result = if claim.quantifier.kind == Kind::Existential {
        if witness.is_some() {
            Outcome::WitnessFound
        } else {
            Outcome::NoWitnessAmongTestedCases
        }
    } else if observations.iter().any(|o| !o.satisfied) {
        Outcome::CounterexampleFound
    } else {
        Outcome::NoCounterexampleInTestedCases
    };
    let failures = observations
        .iter()
        .filter(|o| !o.satisfied)
        .map(|o| o.input.clone())
        .collect();
    let failing_subdomains = finite::collapse(&claim.quantifier.domain, failures, &mut budget)?;
    Ok(Report {
        id: claim.id.clone(),
        text: claim.text.clone(),
        quantifier: claim.quantifier.clone(),
        coverage,
        result,
        observations,
        failing_subdomains,
        witness,
    })
}

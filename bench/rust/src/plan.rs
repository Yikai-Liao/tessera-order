// SPDX-License-Identifier: BSD-2-Clause

//! Shared experiment selection and expansion, independent of measurement scheduling.

mod operation;
pub use operation::Operation;

use serde::{Deserialize, Serialize};

use std::{collections::BTreeSet, sync::OnceLock};
use tessera_order::thresholds::{INSERTION_MAX, TILE_BYTES};

/// The catalog fields needed to expand and validate experiments.
#[derive(Deserialize)]
pub(crate) struct Catalog {
    schema: u32,
    #[serde(rename = "pairedBlocks")]
    pub(crate) paired_blocks: PairedBlocks,
    pub(crate) algorithms: Vec<Algorithm>,
    pub(crate) types: Vec<RecordType>,
    distributions: Vec<Distribution>,
    inputs: InputDefaults,
    #[serde(rename = "revisionSampling")]
    revision_sampling: Vec<RevisionSampling>,
}
/// Supported short and extended paired measurement budgets.
#[derive(Deserialize)]
pub(crate) struct PairedBlocks {
    pub(crate) short: BlockBudget,
    pub(crate) extended: BlockBudget,
}
/// Number of blocks to retain; report statistics own agreement checks.
#[derive(Deserialize)]
pub(crate) struct BlockBudget {
    pub(crate) count: usize,
}
/// Per-revision measurement budget shared with trusted report validation.
#[derive(Deserialize)]
pub struct RevisionSampling {
    /// Inclusive record-count floor.
    pub min_len: usize,
    /// Total warm-up wall time.
    pub warm_ms: u64,
    /// Total sampling wall-time budget, excluding process startup.
    pub window_ms: u64,
    /// Number of adjacent A/B pairs.
    pub samples: usize,
}

/// Fixed prepared-record budget for Criterion batches, shared by both timing harnesses.
/// Batch size does not adapt to measured speed; restoring inputs remains outside the clock.
///
/// # Panics
/// Panics for an empty input.
pub fn criterion_batch_iterations(records_len: usize) -> u64 {
    ((1 << 18) / records_len).max(1) as u64
}

/// Selects the first matching descending length floor from the catalog.
///
/// # Panics
/// Panics for zero records or a catalog without a matching budget.
pub fn revision_sampling(len: usize) -> &'static RevisionSampling {
    catalog()
        .revision_sampling
        .iter()
        .find(|s| len >= s.min_len)
        .expect("revision sampling budget")
}

/// An algorithm's identity, stability and executable adapter family.
#[derive(Deserialize)]
pub(crate) struct Algorithm {
    pub(crate) id: String,
    pub(crate) stable: bool,
    pub(crate) adapter: String,
    #[serde(default)]
    pub(crate) threads: Option<usize>,
}
/// A record's identity and storage width.
#[derive(Deserialize)]
pub(crate) struct RecordType {
    pub(crate) id: String,
    bytes: usize,
}
/// A supported input distribution.
#[derive(Deserialize)]
struct Distribution {
    id: String,
}
/// Default lengths when an inclusion rule omits its record counts.
#[derive(Deserialize)]
struct InputDefaults {
    lengths: Vec<usize>,
}
/// Reads the language-neutral catalog once per executable.
pub(crate) fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let catalog: Catalog =
            toml::from_str(include_str!("../../catalog.toml")).expect("valid benchmark catalog");
        assert_eq!(catalog.schema, 1, "unsupported benchmark catalog schema");
        catalog
    })
}

/// A record count or a boundary resolved from the measured library.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
enum Length {
    Count(usize),
    Boundary(Boundary),
}

/// Engine boundaries whose numeric values may differ between revisions.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Boundary {
    InsertionMax,
    InsertionNext,
    TileNext,
}
impl Length {
    /// Resolves a count using this record representation's byte width.
    fn resolve(&self, ty: &str) -> usize {
        match self {
            Self::Count(len) => *len,
            Self::Boundary(Boundary::InsertionMax) => INSERTION_MAX,
            Self::Boundary(Boundary::InsertionNext) => INSERTION_MAX + 1,
            Self::Boundary(Boundary::TileNext) => {
                let bytes = catalog()
                    .types
                    .iter()
                    .find(|entry| entry.id == ty)
                    .expect("validated record type")
                    .bytes;
                TILE_BYTES / bytes + 1
            }
        }
    }
}

/// A product of input axes; absent axes use the shared catalog.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    #[serde(skip_serializing_if = "Option::is_none")]
    types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    distributions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lengths: Option<Vec<Length>>,
}
impl Rule {
    /// Matches all specified axes, resolving code-relative counts for this type.
    fn matches(&self, ty: &str, distribution: &str, len: usize) -> bool {
        self.types
            .as_ref()
            .is_none_or(|values| values.iter().any(|v| v == ty))
            && self
                .distributions
                .as_ref()
                .is_none_or(|values| values.iter().any(|v| v == distribution))
            && self
                .lengths
                .as_ref()
                .is_none_or(|values| values.iter().any(|v| v.resolve(ty) == len))
    }
}

/// One concrete seed-one input, shared by every selected API mode.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct InputCase {
    /// Record representation from the catalog.
    pub ty: String,
    /// Deterministic key distribution.
    pub distribution: String,
    /// Number of records.
    pub len: usize,
    /// Operation and exact parameters; absent historical inputs mean sorting.
    #[serde(default, skip_serializing_if = "Operation::is_sort")]
    pub operation: Operation,
}

impl InputCase {
    /// Formats the stable identifier used in raw samples and reports.
    pub fn id(&self) -> String {
        format!(
            "{}/{}/{}/s1{}",
            self.ty,
            self.distribution,
            self.len,
            self.operation.suffix()
        )
    }
    /// Names the resumable group containing this case's lengths.
    pub(crate) fn group(&self) -> String {
        format!(
            "{}/{}{}",
            self.ty,
            self.distribution,
            self.operation.suffix()
        )
    }

    /// Small valid input for typed capability checks, never for measurement.
    pub(crate) fn probe(&self) -> Self {
        Self {
            len: 32,
            distribution: "random".into(),
            operation: self.operation.probe(),
            ..self.clone()
        }
    }
}

/// A versioned experiment selection, independent of its measurement protocol.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    _comment: Option<String>,
    schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation: Option<Operation>,
    /// Explicit algorithm selection; measurement drivers own their controls.
    pub(crate) algorithms: Vec<String>,
    include: Vec<Rule>,
    #[serde(default)]
    exclude: Vec<Rule>,
    /// Separate parallel input rules; empty rules reuse the primary selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parallel: Vec<Rule>,
}
impl Plan {
    /// Reads a JSON or TOML selection without choosing a measurement protocol.
    ///
    /// Invalid identities, empty axes and duplicate choices return an error
    /// before input generation or measurement.
    pub fn parse(text: &str) -> Result<Self, String> {
        let plan: Self = serde_json::from_str(text).or_else(|json_error| {
            toml::from_str(text).map_err(|toml_error| {
                format!("invalid plan document\nJSON: {json_error}\nTOML: {toml_error}")
            })
        })?;
        let valid_schema = match (plan.schema, &plan.operation) {
            (1, None) => true,
            (2, Some(operation)) => !operation.is_sort(),
            _ => false,
        };
        if !valid_schema || plan.include.is_empty() || plan.algorithms.is_empty() {
            return Err("expected a nonempty schema 1 plan, or schema 2 with an operation".into());
        }
        let types: Vec<_> = catalog().types.iter().map(|ty| ty.id.clone()).collect();
        let distributions: Vec<_> = catalog()
            .distributions
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let unique = |values: &[String]| {
            !values.is_empty()
                && values
                    .iter()
                    .enumerate()
                    .all(|(index, id)| !values[..index].contains(id))
        };
        if !unique(&plan.algorithms)
            || plan
                .algorithms
                .iter()
                .any(|id| !catalog().algorithms.iter().any(|entry| entry.id == *id))
        {
            return Err("unknown or duplicate algorithm IDs".into());
        }
        for rule in plan
            .include
            .iter()
            .chain(&plan.exclude)
            .chain(&plan.parallel)
        {
            for (values, supported, field) in [
                (&rule.types, &types, "types"),
                (&rule.distributions, &distributions, "distributions"),
            ] {
                if let Some(values) = values
                    && (!unique(values) || values.iter().any(|id| !supported.contains(id)))
                {
                    return Err(format!("unsupported, empty or duplicate {field} selector"));
                }
            }
            if rule.lengths.as_ref().is_some_and(|values| {
                values.is_empty()
                    || values.iter().enumerate().any(|(index, len)| {
                        matches!(len, Length::Count(0)) || values[..index].contains(len)
                    })
            }) {
                return Err("length selectors must be unique, nonempty and positive".into());
            }
        }
        if !plan.operation().is_sort() {
            if plan
                .include
                .iter()
                .chain(&plan.exclude)
                .chain(&plan.parallel)
                .any(|r| {
                    r.lengths.as_ref().is_some_and(|values| {
                        values.iter().any(|n| matches!(n, Length::Boundary(_)))
                    })
                })
            {
                return Err("non-sort plans require literal record counts".into());
            }
            plan.inputs()?;
            plan.parallel_inputs()?;
        }
        Ok(plan)
    }

    /// Returns the requested result contract; old plans remain full sorting.
    pub fn operation(&self) -> Operation {
        self.operation.clone().unwrap_or_default()
    }

    /// Returns exactly the algorithms selected in this plan.
    pub fn algorithms(&self) -> &[String] {
        &self.algorithms
    }

    /// Includes matching products, then removes matching exclusions.
    #[cfg(test)]
    fn selects(&self, ty: &str, distribution: &str, len: usize) -> bool {
        self.include
            .iter()
            .any(|rule| rule.matches(ty, distribution, len))
            && !self
                .exclude
                .iter()
                .any(|rule| rule.matches(ty, distribution, len))
    }

    /// Expands products into unique inputs before allocating any records.
    ///
    /// Omitted axes use catalog identities and default lengths. An empty experiment returns an error.
    pub fn inputs(&self) -> Result<Vec<InputCase>, String> {
        self.expand(&self.include)
    }

    /// Expands the separate parallel selection, or returns empty when none is specified.
    pub fn parallel_inputs(&self) -> Result<Vec<InputCase>, String> {
        if self.parallel.is_empty() {
            return Ok(Vec::new());
        }
        self.expand(&self.parallel)
    }

    /// Applies one API selection and the shared exclusions before generation.
    fn expand(&self, rules: &[Rule]) -> Result<Vec<InputCase>, String> {
        let mut cases = BTreeSet::new();
        for rule in rules {
            let types = rule
                .types
                .clone()
                .unwrap_or_else(|| catalog().types.iter().map(|ty| ty.id.clone()).collect());
            let shapes = rule.distributions.clone().unwrap_or_else(|| {
                catalog()
                    .distributions
                    .iter()
                    .map(|d| d.id.clone())
                    .collect()
            });
            let lengths = rule.lengths.clone().unwrap_or_else(|| {
                catalog()
                    .inputs
                    .lengths
                    .iter()
                    .map(|&len| Length::Count(len))
                    .collect()
            });
            for ty in &types {
                for distribution in &shapes {
                    for length in &lengths {
                        let len = length.resolve(ty);
                        if !self
                            .exclude
                            .iter()
                            .any(|rule| rule.matches(ty, distribution, len))
                        {
                            let operation = self.operation();
                            operation.validate(len)?;
                            cases.insert(InputCase {
                                ty: ty.clone(),
                                distribution: distribution.clone(),
                                len,
                                operation,
                            });
                        }
                    }
                }
            }
        }
        if cases.is_empty() {
            return Err("the benchmark plan selects no supported cases".into());
        }
        Ok(cases.into_iter().collect())
    }
}

/// Formats a seed-one Criterion group from a typed representation's format.
///
/// The representation format stays literal, so row prefixes and constant names
/// use one allocation, without intermediate strings or formatting calls.
/// Identity validation belongs to the plan, not this formatter.
///
/// ```
/// assert_eq!(tessera_bench::plan::criterion_group!("{}", "u64"; "random", 1000),
///            "rust/u64/random/1000/s1");
/// assert_eq!(tessera_bench::plan::criterion_group!("row_{}", "u64"; "random", 1000),
///            "rust/row_u64/random/1000/s1");
/// assert_eq!(tessera_bench::plan::criterion_group!("rec128_u64"; "random", 1000),
///            "rust/rec128_u64/random/1000/s1");
/// ```
#[macro_export]
macro_rules! criterion_group {
    ($ty:literal $(, $argument:expr)*; $distribution:expr, $len:expr) => {
        ::std::format!(
            ::std::concat!("rust/", $ty, "/{distribution}/{len}/s1"),
            $($argument,)*
            distribution = $distribution,
            len = $len,
        )
    };
}

/// Joins a Criterion group and API mode for exact timed-ID selection.
///
/// This matches Criterion's identity without parsing or validating its fields.
///
/// ```
/// assert_eq!(tessera_bench::plan::criterion_id!("rust/u64/random/1000/s1", "api"),
///            "rust/u64/random/1000/s1/api");
/// ```
#[macro_export]
macro_rules! criterion_id {
    ($group:expr, $mode:expr) => {
        ::std::format!("{id}/{mode}", id = $group, mode = $mode)
    };
}

/// Criterion formatters share this owner with comparison input identities.
pub use crate::{criterion_group, criterion_id};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn selection_rules_match_contract() {
        let fixture: Value =
            toml::from_str(include_str!("../../plans/selection.fixture.toml")).unwrap();
        let plan = Plan::parse(&fixture["plan"].to_string()).unwrap();
        for input in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                plan.selects(
                    input["type"].as_str().unwrap(),
                    input["distribution"].as_str().unwrap(),
                    input["length"].as_u64().unwrap() as usize
                ),
                input["selected"].as_bool().unwrap(),
                "{input}"
            );
        }
    }

    #[test]
    fn identities_match_shared_contract() {
        let fixture: Value =
            toml::from_str(include_str!("../../plans/selection.fixture.toml")).unwrap();
        for entry in fixture["identities"].as_array().unwrap() {
            let case = InputCase {
                ty: entry["type"].as_str().unwrap().into(),
                distribution: entry["distribution"].as_str().unwrap().into(),
                len: entry["length"].as_u64().unwrap() as usize,
                operation: Operation::Sort {},
            };
            let group = criterion_group!("{}", &case.ty; &case.distribution, case.len);
            assert_eq!(group, entry["criterion_group"].as_str().unwrap());
            assert_eq!(
                criterion_id!(&group, entry["mode"].as_str().unwrap()),
                entry["criterion"].as_str().unwrap()
            );
            assert_eq!(case.id(), entry["comparison"].as_str().unwrap());
        }
    }

    #[test]
    fn operation_grammar_and_ids_match_shared_contract() {
        let fixture: Value =
            toml::from_str(include_str!("../../plans/selection.fixture.toml")).unwrap();
        let input = &fixture["operation_case"];
        for entry in fixture["operations"].as_array().unwrap() {
            let plan = serde_json::json!({
                "schema": 2, "algorithms": ["tessera"], "operation": entry["operation"],
                "include": [{"types": [input["ty"]], "distributions": [input["distribution"]], "lengths": [input["len"]]}]
            });
            let parsed = Plan::parse(&plan.to_string());
            if let Some(id) = entry["comparison"].as_str() {
                let cases = parsed.unwrap().inputs().unwrap();
                assert_eq!(cases.len(), 1);
                assert_eq!(cases[0].id(), id);
                assert_eq!(cases[0].group(), entry["group"].as_str().unwrap());
            } else {
                assert!(parsed.is_err(), "{entry}");
            }
        }
    }

    #[test]
    fn presets_preserve_coverage() {
        for (text, expected) in [
            (include_str!("../../plans/continuous.toml"), 62),
            (include_str!("../../plans/smoke.toml"), 125),
            (include_str!("../../plans/release.toml"), 381),
            (include_str!("../../plans/parallel.toml"), 16),
        ] {
            assert_eq!(Plan::parse(text).unwrap().inputs().unwrap().len(), expected);
        }
        let pairs = |text| {
            Plan::parse(text)
                .unwrap()
                .inputs()
                .unwrap()
                .into_iter()
                .map(|case| (case.ty, case.distribution))
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            pairs(include_str!("../../plans/smoke.toml")),
            pairs(include_str!("../../plans/release.toml"))
        );
    }

    #[test]
    fn continuous_preserves_required_screens() {
        let continuous = Plan::parse(include_str!("../../plans/continuous.toml")).unwrap();
        let cases = continuous.inputs().unwrap();
        let require = |inputs: &[InputCase], ty: &str, distribution: &str, len: usize| {
            assert!(
                inputs.iter().any(|case| {
                    case.ty == ty && case.distribution == distribution && case.len == len
                }),
                "missing required continuous workload: {ty}/{distribution}/{len}/s1"
            );
        };
        // Policy identities must survive equal-count substitutions in the plan.
        for ty in ["u64", "row_u64"] {
            for distribution in ["random", "sorted", "reverse", "equal"] {
                require(&cases, ty, distribution, INSERTION_MAX);
            }
        }
        for ty in ["u64", "row_u32"] {
            require(&cases, ty, "random", INSERTION_MAX + 1);
        }
        for (ty, bytes) in [("u64", 8), ("row_u32", 8), ("row_u64", 16)] {
            require(&cases, ty, "random", TILE_BYTES / bytes + 1);
        }
        for ty in ["u64", "u32", "f64", "row_u64"] {
            for distribution in ["sorted", "reverse"] {
                require(&cases, ty, distribution, 1_000);
            }
        }
        require(&cases, "u64", "equal", 1_000);
        for ty in ["u64", "row_u64"] {
            for distribution in ["random", "skew99"] {
                require(&cases, ty, distribution, 1_000);
            }
        }
        for (ty, len) in [("u64", 65_536), ("row_u64", 65_536), ("u64", 1_000_000)] {
            require(&cases, ty, "sorted_tail", len);
        }
        for ty in ["u32", "u64"] {
            require(&cases, ty, "sorted", 65_536);
        }
        for distribution in [
            "d16",
            "d4096",
            "sparse_digits",
            "zipf2",
            "skew99",
            "two_values",
        ] {
            require(&cases, "u64", distribution, 65_536);
        }
        for (ty, distribution) in [
            ("f64", "finite_signed"),
            ("row_u64", "d16"),
            ("row_u64", "hot50"),
            ("rec128_u64", "sparse_digits"),
        ] {
            require(&cases, ty, distribution, 65_536);
        }
        for ty in ["u32", "u64", "u128", "f64", "row_u64", "rec128_u64"] {
            require(&cases, ty, "random", 65_536);
        }
        for ty in ["u32", "u64", "u128", "row_u32", "row_u64"] {
            require(&cases, ty, "random", 1_000_000);
        }
        require(&cases, "u128", "sparse8", 1_000_000);
        require(&cases, "u64", "bimodal", 1_000_000);
        // Bound the expensive serial phase without locking input iteration order.
        assert_eq!(cases.iter().filter(|case| case.len >= 1_000_000).count(), 8);
        assert!(!cases.iter().any(|case| {
            case.ty == "u64"
                && ["skew99", "two_values"].contains(&case.distribution.as_str())
                && case.len >= 1_000_000
        }));

        let parallel = continuous.parallel_inputs().unwrap();
        assert_eq!(parallel.len(), 36);
        assert_eq!(
            parallel.iter().filter(|case| case.len == 1_000_000).count(),
            32
        );
        assert!(cases.iter().all(|case| case.len <= 1_000_000));
        assert!(parallel.iter().all(|case| case.len <= 1_000_000));
        for ty in ["u32", "row_u32", "u64", "row_u64"] {
            require(&parallel, ty, "random", 65_536);
        }
        for ty in [
            "u32",
            "u64",
            "u128",
            "f64",
            "row_u32",
            "row_u64",
            "rec128_u64",
        ] {
            require(&parallel, ty, "random", 1_000_000);
        }
        for distribution in [
            "sorted",
            "reverse",
            "sorted99",
            "equal",
            "far_swaps",
            "d16",
            "d4096",
            "sparse_digits",
            "zipf2",
            "skew99",
            "two_values",
            "sorted_tail",
            "runs",
            "bimodal",
        ] {
            require(&parallel, "u64", distribution, 1_000_000);
        }
        for distribution in [
            "sorted",
            "reverse",
            "equal",
            "d16",
            "hot50",
            "skew99",
            "sorted_tail",
        ] {
            require(&parallel, "row_u64", distribution, 1_000_000);
        }
        for (ty, distribution) in [
            ("row_u32", "skew99"),
            ("u128", "sparse8"),
            ("f64", "finite_signed"),
            ("rec128_u64", "sparse_digits"),
        ] {
            require(&parallel, ty, distribution, 1_000_000);
        }
    }

    #[test]
    fn relative_lengths_follow_records() {
        let plan = Plan::parse(r#"{"schema":1,"algorithms":["tessera"],
            "include":[{"types":["u64","row_u64"],"distributions":["random"],"lengths":["tile-next"]}]}"#).unwrap();
        for case in plan.inputs().unwrap() {
            let bytes = if case.ty == "u64" { 8 } else { 16 };
            assert_eq!(case.len, TILE_BYTES / bytes + 1);
        }
    }
}

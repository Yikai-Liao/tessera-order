// SPDX-License-Identifier: BSD-2-Clause

//! Shared experiment selection and expansion, independent of measurement scheduling.

mod execution;
mod operation;
pub use execution::Execution;
pub use operation::Operation;

use serde::{Deserialize, Serialize};

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
    sync::OnceLock,
};
use tessera_order::thresholds::{INSERTION_MAX, TILE_BYTES};

/// The catalog fields needed to expand and validate experiments.
#[derive(Deserialize)]
pub(crate) struct Catalog {
    schema: u32,
    #[serde(rename = "pairedBlocks")]
    pub(crate) paired_blocks: PairedBlocks,
    pub(crate) implementations: Vec<Implementation>,
    pub(crate) types: Vec<RecordType>,
    distributions: Vec<Distribution>,
    operations: Vec<OperationEntry>,
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

/// An implementation's identity, route contracts and executable adapter family.
#[derive(Deserialize)]
pub(crate) struct Implementation {
    pub(crate) id: String,
    stable: bool,
    pub(crate) adapter: String,
    /// A parallel full-sort entry point, if the implementation has one.
    pub(crate) parallel: Option<ParallelSort>,
    /// Non-sort call metadata, keyed by operation kind.
    #[serde(default)]
    pub(crate) routes: std::collections::BTreeMap<String, Route>,
}
impl Implementation {
    /// Resolves the operation's stability, including routes that inherit the default.
    pub(crate) fn route_stable(&self, operation: &Operation) -> bool {
        self.routes
            .get(operation.kind())
            .and_then(|route| route.stable)
            .unwrap_or(self.stable)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Route {
    /// Display detail used by TypeScript; Rust reports the exact typed call.
    #[serde(rename = "call")]
    _call: String,
    stable: Option<bool>,
}
/// Where a parallel full-sort entry point stands beside the serial entry points.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ParallelSort {
    /// Beside the serial entry points.
    Also,
    /// Instead of every serial entry point.
    Only,
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
/// An operation's identity and the order of its identity parameters.
#[derive(Deserialize)]
struct OperationEntry {
    id: String,
    /// The implementation whose serial column every case of this operation pairs as a control.
    reference: String,
    #[serde(default)]
    parameters: Vec<Parameter>,
}
/// One integer operation parameter, named as in plans and manifests.
#[derive(Deserialize)]
struct Parameter {
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
        assert_eq!(catalog.schema, 3, "unsupported benchmark catalog schema");
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
            Self::Boundary(Boundary::TileNext) => TILE_BYTES / record_bytes(ty) + 1,
        }
    }
}

/// A product of case and execution axes. Absent include axes use the
/// catalog, `sort` and `serial`; absent exclude axes match every value.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    #[serde(skip_serializing_if = "Option::is_none")]
    types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    distributions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lengths: Option<Vec<Length>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    executions: Option<Vec<Execution>>,
}
impl Rule {
    /// Matches all specified axes, resolving code-relative counts for this type.
    fn matches(&self, case: &InputCase, execution: Execution) -> bool {
        self.types
            .as_ref()
            .is_none_or(|values| values.contains(&case.ty))
            && self
                .distributions
                .as_ref()
                .is_none_or(|values| values.contains(&case.distribution))
            && self
                .lengths
                .as_ref()
                .is_none_or(|values| values.iter().any(|v| v.resolve(&case.ty) == case.len))
            && self
                .operation
                .as_ref()
                .is_none_or(|operation| *operation == case.operation)
            && self
                .executions
                .as_ref()
                .is_none_or(|values| values.contains(&execution))
    }
}

/// Storage width of a validated catalog record type.
fn record_bytes(ty: &str) -> usize {
    catalog()
        .types
        .iter()
        .find(|entry| entry.id == ty)
        .expect("validated record type")
        .bytes
}

/// The catalog entry of an operation kind.
fn operation_entry(kind: &str) -> Option<&'static OperationEntry> {
    catalog().operations.iter().find(|entry| entry.id == kind)
}

/// The implementation every serial case of an operation pairs with Tessera as its control.
pub(crate) fn operation_reference(operation: &Operation) -> &'static str {
    &operation_entry(operation.kind())
        .expect("catalog operation")
        .reference
}

/// Decodes a canonical nonnegative integer identity component.
fn parse_count(text: &str) -> Option<usize> {
    (text == "0" || (!text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit())))
        .then(|| text.parse().ok())
        .flatten()
}

/// One operation on one concrete seed-one input: the pairing unit that every
/// column of a session measures in the same blocks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct InputCase {
    /// Record representation from the catalog.
    pub ty: String,
    /// Deterministic key distribution.
    pub distribution: String,
    /// Number of records.
    pub len: usize,
    /// Operation and exact parameters.
    pub operation: Operation,
}

impl InputCase {
    /// The operation kind and its parameters in catalog order.
    fn operation_parts(&self) -> (&'static str, Vec<usize>) {
        let value = serde_json::to_value(&self.operation).expect("operation serializes");
        let entry = operation_entry(self.operation.kind()).expect("catalog operation");
        let parameters = entry
            .parameters
            .iter()
            .map(|parameter| value[&parameter.id].as_u64().expect("integer parameter") as usize)
            .collect();
        (&entry.id, parameters)
    }

    /// Formats `operation/type/distribution/length/s1[/parameters]`, used in
    /// raw samples, memory rows and reports.
    pub fn id(&self) -> String {
        let (kind, parameters) = self.operation_parts();
        let mut id = format!("{kind}/{}/{}/{}/s1", self.ty, self.distribution, self.len);
        for parameter in parameters {
            id += &format!("/{parameter}");
        }
        id
    }

    /// Decodes a case ID with catalog identities and valid parameters.
    ///
    /// # Errors
    /// Returns an error for any noncanonical or unknown component.
    pub fn parse_id(id: &str) -> Result<Self, String> {
        let invalid = || format!("invalid case ID {id:?}");
        let parts: Vec<_> = id.split('/').collect();
        let entry = operation_entry(parts[0]).ok_or_else(invalid)?;
        if parts.len() != 5 + entry.parameters.len()
            || !catalog().types.iter().any(|ty| ty.id == parts[1])
            || !catalog().distributions.iter().any(|d| d.id == parts[2])
            || parts[4] != "s1"
        {
            return Err(invalid());
        }
        let len = parse_count(parts[3])
            .filter(|&len| len > 0)
            .ok_or_else(invalid)?;
        let mut operation = serde_json::Map::new();
        operation.insert("kind".into(), entry.id.clone().into());
        for (parameter, text) in entry.parameters.iter().zip(&parts[5..]) {
            operation.insert(
                parameter.id.clone(),
                parse_count(text).ok_or_else(invalid)?.into(),
            );
        }
        let case = Self {
            ty: parts[1].into(),
            distribution: parts[2].into(),
            len,
            operation: serde_json::from_value(operation.into()).map_err(|_| invalid())?,
        };
        case.operation.validate(len)?;
        Ok(case)
    }

    /// Formats the Criterion group `rust/<case>`, whose functions are executions.
    pub fn criterion_group(&self) -> String {
        format!("rust/{}", self.id())
    }

    /// Formats the Criterion ID `rust/<case>/<execution>`.
    pub fn criterion_id(&self, execution: Execution) -> String {
        format!("{}/{execution}", self.criterion_group())
    }

    /// Decodes a Criterion ID into its case and execution.
    ///
    /// # Errors
    /// Returns an error for a missing prefix or an invalid case or execution.
    pub fn parse_criterion_id(id: &str) -> Result<(Self, Execution), String> {
        let (case, execution) = id
            .strip_prefix("rust/")
            .and_then(|rest| rest.rsplit_once('/'))
            .ok_or_else(|| format!("invalid Criterion ID {id:?}"))?;
        Ok((Self::parse_id(case)?, execution.parse()?))
    }

    /// Bytes of the input records, excluding owned payloads.
    pub fn input_bytes(&self) -> usize {
        self.len * record_bytes(&self.ty)
    }

    /// Names the resumable group `operation/type/distribution[/parameters]`,
    /// which holds every length of one result contract.
    pub fn group(&self) -> String {
        let (kind, parameters) = self.operation_parts();
        let mut group = format!("{kind}/{}/{}", self.ty, self.distribution);
        for parameter in parameters {
            group += &format!("/{parameter}");
        }
        group
    }

    /// Small valid input for typed capability checks, never for measurement.
    pub fn probe(&self) -> Self {
        Self {
            len: 32,
            distribution: "random".into(),
            operation: self.operation.probe(),
            ..self.clone()
        }
    }
}

/// What a session measures on a case: one implementation at one execution.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Column {
    /// Catalog implementation identity.
    pub implementation: String,
    /// Serial or parallel entry point and its worker count.
    pub execution: Execution,
}

impl fmt::Display for Column {
    /// Formats `implementation/execution`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.implementation, self.execution)
    }
}

impl FromStr for Column {
    type Err = String;

    /// Decodes a column with a catalog implementation.
    fn from_str(id: &str) -> Result<Self, String> {
        let (implementation, execution) = id
            .split_once('/')
            .filter(|(implementation, _)| {
                catalog()
                    .implementations
                    .iter()
                    .any(|entry| entry.id == *implementation)
            })
            .ok_or_else(|| format!("invalid column ID {id:?}"))?;
        Ok(Self {
            implementation: implementation.into(),
            execution: execution.parse()?,
        })
    }
}

/// A versioned experiment selection, independent of its measurement protocol.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    _comment: Option<String>,
    schema: u32,
    /// Explicit implementation selection; measurement drivers own their controls.
    pub(crate) implementations: Vec<String>,
    include: Vec<Rule>,
    #[serde(default)]
    exclude: Vec<Rule>,
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
        if plan.schema != 3 {
            return Err(format!(
                "unsupported plan schema {}; regenerate the plan for schema 3",
                plan.schema
            ));
        }
        if plan.include.is_empty() || plan.implementations.is_empty() {
            return Err("a plan needs implementations and include rules".into());
        }
        let types: Vec<_> = catalog().types.iter().map(|ty| ty.id.clone()).collect();
        let distributions: Vec<_> = catalog()
            .distributions
            .iter()
            .map(|d| d.id.clone())
            .collect();
        fn unique<T: PartialEq>(values: &[T]) -> bool {
            !values.is_empty()
                && values
                    .iter()
                    .enumerate()
                    .all(|(index, id)| !values[..index].contains(id))
        }
        if !unique(&plan.implementations)
            || plan.implementations.iter().any(|id| {
                !catalog()
                    .implementations
                    .iter()
                    .any(|entry| entry.id == *id)
            })
        {
            return Err("unknown or duplicate implementation IDs".into());
        }
        for rule in plan.include.iter().chain(&plan.exclude) {
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
                !unique(values) || values.iter().any(|len| matches!(len, Length::Count(0)))
            }) {
                return Err("length selectors must be unique, nonempty and positive".into());
            }
            if rule
                .executions
                .as_ref()
                .is_some_and(|values| !unique(values))
            {
                return Err("execution selectors must be unique and nonempty".into());
            }
        }
        // Operation parameters are checked against literal counts in both languages.
        if plan.include.iter().any(|rule| {
            rule.operation.as_ref().is_some_and(|o| !o.is_sort())
                && rule
                    .lengths
                    .as_ref()
                    .is_some_and(|values| values.iter().any(|n| matches!(n, Length::Boundary(_))))
        }) {
            return Err("non-sort rules require literal record counts".into());
        }
        plan.cases()?;
        Ok(plan)
    }

    /// Returns exactly the implementations selected in this plan.
    pub fn implementations(&self) -> &[String] {
        &self.implementations
    }

    /// Whether a case at one execution survives the include and exclude rules.
    #[cfg(test)]
    fn selects(&self, case: &InputCase, execution: Execution) -> bool {
        self.cases()
            .unwrap()
            .iter()
            .any(|(selected, executions)| selected == case && executions.contains(&execution))
    }

    /// Expands rules into unique cases, each with its selected executions in
    /// ascending order, before allocating any records.
    ///
    /// Omitted axes use catalog identities, default lengths, `sort` and
    /// `serial`. Invalid operation parameters or an empty experiment return an error.
    pub fn cases(&self) -> Result<Vec<(InputCase, Vec<Execution>)>, String> {
        let mut cases = BTreeMap::<InputCase, BTreeSet<Execution>>::new();
        for rule in &self.include {
            let types = rule
                .types
                .clone()
                .unwrap_or_else(|| catalog().types.iter().map(|ty| ty.id.clone()).collect());
            let distribution_ids = rule.distributions.clone().unwrap_or_else(|| {
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
            let operation = rule.operation.clone().unwrap_or_default();
            let executions = rule
                .executions
                .clone()
                .unwrap_or_else(|| vec![Execution::Serial]);
            for ty in &types {
                for distribution in &distribution_ids {
                    for length in &lengths {
                        let case = InputCase {
                            ty: ty.clone(),
                            distribution: distribution.clone(),
                            len: length.resolve(ty),
                            operation: operation.clone(),
                        };
                        let surviving: Vec<_> = executions
                            .iter()
                            .copied()
                            .filter(|&e| !self.exclude.iter().any(|rule| rule.matches(&case, e)))
                            .collect();
                        if !surviving.is_empty() {
                            case.operation.validate(case.len)?;
                            cases.entry(case).or_default().extend(surviving);
                        }
                    }
                }
            }
        }
        if cases.is_empty() {
            return Err("the benchmark plan selects no supported cases".into());
        }
        Ok(cases
            .into_iter()
            .map(|(case, executions)| (case, executions.into_iter().collect()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        toml::from_str(include_str!("../../plans/selection.fixture.toml")).unwrap()
    }

    /// Builds a fixture case, whose absent operation means sort.
    fn fixture_case(entry: &Value) -> InputCase {
        InputCase {
            ty: entry["type"].as_str().unwrap().into(),
            distribution: entry["distribution"].as_str().unwrap().into(),
            len: entry["length"].as_u64().unwrap() as usize,
            operation: entry.get("operation").map_or_else(Operation::default, |o| {
                serde_json::from_value(o.clone()).unwrap()
            }),
        }
    }

    #[test]
    fn selection_rules_match_contract() {
        let fixture = fixture();
        let plan = Plan::parse(&fixture["plan"].to_string()).unwrap();
        for entry in fixture["cases"].as_array().unwrap() {
            let execution = entry
                .get("execution")
                .map_or(Execution::Serial, |e| e.as_str().unwrap().parse().unwrap());
            assert_eq!(
                plan.selects(&fixture_case(entry), execution),
                entry["selected"].as_bool().unwrap(),
                "{entry}"
            );
        }
    }

    #[test]
    fn identities_match_shared_contract() {
        let fixture = fixture();
        for entry in fixture["identities"].as_array().unwrap() {
            let case = fixture_case(&entry["case"]);
            let column = Column {
                implementation: entry["column"]["implementation"].as_str().unwrap().into(),
                execution: entry["column"]["execution"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
            };
            let id = entry["case_id"].as_str().unwrap();
            let criterion = entry["criterion"].as_str().unwrap();
            assert_eq!(case.id(), id);
            assert_eq!(case.group(), entry["group"].as_str().unwrap());
            assert_eq!(column.to_string(), entry["column_id"].as_str().unwrap());
            assert_eq!(case.criterion_id(column.execution), criterion);
            assert_eq!(InputCase::parse_id(id).unwrap(), case);
            assert_eq!(
                entry["column_id"]
                    .as_str()
                    .unwrap()
                    .parse::<Column>()
                    .unwrap(),
                column
            );
            assert_eq!(
                InputCase::parse_criterion_id(criterion).unwrap(),
                (case, column.execution)
            );
        }
        let invalid = &fixture["invalid"];
        let texts = |field: &str| {
            invalid[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        for text in texts("executions") {
            assert!(text.parse::<Execution>().is_err(), "{text}");
        }
        for text in texts("columns") {
            assert!(text.parse::<Column>().is_err(), "{text}");
        }
        for text in texts("case_ids") {
            assert!(InputCase::parse_id(&text).is_err(), "{text}");
        }
        for text in texts("criterion") {
            assert!(InputCase::parse_criterion_id(&text).is_err(), "{text}");
        }
    }

    #[test]
    fn operation_grammar_and_ids_match_shared_contract() {
        let fixture = fixture();
        let input = &fixture["operation_case"];
        for entry in fixture["operations"].as_array().unwrap() {
            let plan = json!({
                "schema": 3, "implementations": ["tessera"],
                "include": [{
                    "types": [input["type"]], "distributions": [input["distribution"]],
                    "lengths": [input["length"]], "operation": entry["operation"]
                }]
            });
            let parsed = Plan::parse(&plan.to_string());
            if let Some(id) = entry["case_id"].as_str() {
                let cases = parsed.unwrap().cases().unwrap();
                assert_eq!(cases.len(), 1);
                assert_eq!(cases[0].0.id(), id);
            } else {
                assert!(parsed.is_err(), "{entry}");
            }
        }
    }

    #[test]
    fn catalog_names_every_operation() {
        let ids: Vec<_> = catalog().operations.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["sort", "argsort", "select_nth", "partition-buckets"]);
        for operation in [
            Operation::Sort {},
            Operation::Argsort {},
            Operation::SelectNth { index: 1 },
            Operation::PartitionBuckets { bucket_count: 2 },
        ] {
            assert_eq!(
                serde_json::to_value(&operation).unwrap()["kind"],
                operation.kind()
            );
            let case = InputCase {
                ty: "u64".into(),
                distribution: "random".into(),
                len: 4,
                operation,
            };
            assert_eq!(InputCase::parse_id(&case.id()).unwrap(), case);
        }
    }

    #[test]
    fn old_schemas_are_rejected() {
        for schema in [1, 2] {
            let plan = json!({
                "schema": schema, "implementations": ["tessera"],
                "include": [{"types": ["u64"], "distributions": ["random"], "lengths": [1000]}]
            });
            let error = Plan::parse(&plan.to_string()).unwrap_err();
            assert!(error.contains("schema 3"), "{error}");
        }
    }

    /// Counts measured (case, execution) pairs per preset.
    fn measurements(text: &str) -> usize {
        let plan = Plan::parse(text).unwrap();
        plan.cases().unwrap().iter().map(|(_, e)| e.len()).sum()
    }

    #[test]
    fn presets_preserve_coverage() {
        for (text, cases, measured) in [
            (include_str!("../../plans/continuous.toml"), 108, 119),
            (include_str!("../../plans/smoke.toml"), 125, 125),
            (include_str!("../../plans/release.toml"), 381, 381),
            (include_str!("../../plans/parallel.toml"), 16, 32),
        ] {
            assert_eq!(Plan::parse(text).unwrap().cases().unwrap().len(), cases);
            assert_eq!(measurements(text), measured);
        }
        let pairs = |text| {
            Plan::parse(text)
                .unwrap()
                .cases()
                .unwrap()
                .into_iter()
                .map(|(case, _)| (case.ty, case.distribution))
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
        let all = continuous.cases().unwrap();
        let at = |execution: Execution| {
            all.iter()
                .filter(|(_, e)| e.contains(&execution))
                .map(|(case, _)| case.clone())
                .collect::<Vec<_>>()
        };
        let cases = at(Execution::Serial);
        let parallel = at("t4".parse().unwrap());
        assert_eq!(cases.len(), 83);
        let require = |inputs: &[InputCase], ty: &str, distribution: &str, len: usize| {
            assert!(
                inputs.iter().any(|case| {
                    case.operation.is_sort()
                        && case.ty == ty
                        && case.distribution == distribution
                        && case.len == len
                }),
                "missing required continuous input: sort/{ty}/{distribution}/{len}/s1"
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

        #[cfg(feature = "operations")]
        let tessera = crate::list_variants()
            .into_iter()
            .find(|variant| variant.name == "tessera")
            .unwrap();
        for id in [
            "partition-buckets/u64/random/65536/s1/16",
            "partition-buckets/row_u64/random/65536/s1/16",
            "partition-buckets/u64/random/65536/s1/65537",
            "partition-buckets/row_u64/random/65536/s1/65537",
            "argsort/row_u64/d16/65536/s1",
            "select_nth/row_u64/d16/65536/s1/32768",
            "select_nth/row_u64/d16/65536/s1/0",
            "select_nth/row_u64/d16/65536/s1/65535",
            "select_nth/rec128_u64/random/65536/s1/0",
            "select_nth/rec128_u64/random/65536/s1/65535",
        ] {
            let case = cases.iter().find(|case| case.id() == id);
            assert!(case.is_some(), "missing required continuous input: {id}");
            #[cfg(feature = "operations")]
            assert!(
                tessera.supports(
                    &crate::CaseInput::new(&case.unwrap().probe()),
                    Execution::Serial
                ),
                "unsupported continuous input: {id}"
            );
        }

        // Bound the expensive serial phase without locking input iteration order.
        assert_eq!(cases.iter().filter(|case| case.len >= 1_000_000).count(), 9);
        assert!(!cases.iter().any(|case| {
            case.ty == "u64"
                && ["skew99", "two_values"].contains(&case.distribution.as_str())
                && case.len >= 1_000_000
        }));

        assert_eq!(parallel.len(), 36);
        assert!(parallel.iter().all(|case| case.operation.is_sort()));
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
    fn rules_combine_operations_and_executions() {
        let rule = json!({"types":["u64"],"distributions":["random"],"lengths":[1000],
            "executions":["serial","t2"]});
        let select = json!({"types":["u64"],"distributions":["random"],"lengths":[1000],
            "operation":{"kind":"select_nth","index":10}});
        let plan = |include: Value, exclude: Value| {
            Plan::parse(
                &json!({"schema":3,"implementations":["tessera"],"include":include,"exclude":exclude})
                    .to_string(),
            )
        };
        let ids = |plan: Plan| {
            plan.cases()
                .unwrap()
                .iter()
                .flat_map(|(case, executions)| executions.iter().map(|e| case.criterion_id(*e)))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(plan(json!([rule, select]), json!([])).unwrap()),
            [
                "rust/sort/u64/random/1000/s1/serial",
                "rust/sort/u64/random/1000/s1/t2",
                "rust/select_nth/u64/random/1000/s1/10/serial",
            ]
        );
        // Exclusions may name any axis, including operations and executions.
        assert_eq!(
            ids(plan(json!([rule, select]), json!([{"executions":["serial"]}])).unwrap()),
            ["rust/sort/u64/random/1000/s1/t2"]
        );
        assert_eq!(
            ids(plan(
                json!([rule, select]),
                json!([{"operation":{"kind":"sort"}}])
            )
            .unwrap()),
            ["rust/select_nth/u64/random/1000/s1/10/serial"]
        );
        let mut short = select.clone();
        short["lengths"] = json!([5]);
        assert!(plan(json!([short]), json!([])).is_err());
        let mut relative = select.clone();
        relative["lengths"] = json!(["tile-next"]);
        assert!(plan(json!([relative]), json!([])).is_err());
        let mut duplicate = rule.clone();
        duplicate["executions"] = json!(["t2", "t2"]);
        assert!(plan(json!([duplicate]), json!([])).is_err());
        assert!(plan(json!([rule]), json!([rule])).is_err());
    }

    #[test]
    fn relative_lengths_follow_records() {
        let plan = Plan::parse(r#"{"schema":3,"implementations":["tessera"],
            "include":[{"types":["u64","row_u64"],"distributions":["random"],"lengths":["tile-next"]}]}"#).unwrap();
        for (case, _) in plan.cases().unwrap() {
            let bytes = if case.ty == "u64" { 8 } else { 16 };
            assert_eq!(case.len, TILE_BYTES / bytes + 1);
        }
    }
}

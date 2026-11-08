// SPDX-License-Identifier: BSD-2-Clause

//! Experiment selection and expansion for both benchmark harnesses.

use serde::{Deserialize, Serialize};

use std::{collections::BTreeSet, sync::OnceLock};
use tessera_sort::thresholds::{INSERTION_MAX, TILE_BYTES};

/// The catalog fields needed to expand and validate experiments.
#[derive(Deserialize)]
pub(crate) struct Catalog {
    schema: u32,
    pub(crate) algorithms: Vec<Algorithm>,
    types: Vec<RecordType>,
    smoke: Inventory,
    pub(crate) comparison: Inventory,
}
/// An algorithm's identity, stability and executable adapter family.
#[derive(Deserialize)]
pub(crate) struct Algorithm {
    pub(crate) id: String,
    #[cfg(feature = "compare")]
    pub(crate) stable: bool,
    #[cfg(feature = "compare")]
    pub(crate) adapter: String,
}
/// A record's identity and storage width.
#[derive(Deserialize)]
struct RecordType {
    id: String,
    bytes: usize,
}
/// Defaults and supported identities for one harness.
#[derive(Deserialize)]
pub(crate) struct Inventory {
    pub(crate) types: Vec<String>,
    shapes: Vec<String>,
    lengths: Vec<usize>,
    modes: Vec<String>,
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

/// A product of input axes; absent axes use the harness's catalog.
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
    modes: Option<Vec<String>>,
}
impl Rule {
    /// Matches all specified axes, resolving code-relative counts for this type.
    fn matches(&self, ty: &str, distribution: &str, len: usize, mode: &str) -> bool {
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
            && self
                .modes
                .as_ref()
                .is_none_or(|values| values.iter().any(|v| v == mode))
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
}

#[cfg(feature = "compare")]
impl InputCase {
    /// Formats the stable identifier used in raw samples and reports.
    pub(crate) fn id(&self) -> String {
        format!("{}/{}/{}/s1", self.ty, self.distribution, self.len)
    }
    /// Names the resumable group containing this case's lengths.
    pub(crate) fn group(&self) -> String {
        format!("{}/{}", self.ty, self.distribution)
    }
}

/// A versioned experiment selection, independent of its measurement protocol.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    _comment: Option<String>,
    schema: u32,
    /// Harness used to expand omitted axes.
    harness: String,
    /// Algorithm IDs, including the measurement protocol's controls.
    pub(crate) algorithms: Vec<String>,
    include: Vec<Rule>,
    #[serde(default)]
    exclude: Vec<Rule>,
}
impl Plan {
    /// Reads a JSON or TOML experiment, retaining its protocol's controls.
    ///
    /// Invalid identities, empty axes, duplicate choices and unsupported harness
    /// features return an error before input generation or measurement.
    pub fn parse(text: &str, harness: &str) -> Result<Self, String> {
        let mut plan: Self = serde_json::from_str(text).or_else(|json_error| {
            toml::from_str(text).map_err(|toml_error| {
                format!("invalid plan document\nJSON: {json_error}\nTOML: {toml_error}")
            })
        })?;
        if plan.schema != 1
            || plan.harness != harness
            || !["smoke", "comparison"].contains(&harness)
            || plan.include.is_empty()
            || plan.algorithms.is_empty()
        {
            return Err(format!("expected a nonempty schema 1 {harness} plan"));
        }
        let inventory = plan.inventory();
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
        for rule in plan.include.iter().chain(&plan.exclude) {
            for (values, supported, field) in [
                (&rule.types, &inventory.types, "types"),
                (&rule.distributions, &inventory.shapes, "distributions"),
                (&rule.modes, &inventory.modes, "modes"),
            ] {
                if let Some(values) = values {
                    if !unique(values) || values.iter().any(|id| !supported.contains(id)) {
                        return Err(format!("unsupported, empty or duplicate {field} selector"));
                    }
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
        if harness == "smoke" && plan.algorithms.iter().any(|id| id != "tessera") {
            return Err("the smoke harness measures Tessera only".into());
        }
        let controls = if harness == "comparison" {
            &["tessera", "std-stable"][..]
        } else {
            &["tessera"][..]
        };
        plan.algorithms
            .retain(|id| !controls.contains(&id.as_str()));
        plan.algorithms
            .splice(0..0, controls.iter().map(|id| (*id).into()));
        Ok(plan)
    }

    /// Names the defaults validated when this plan was parsed.
    fn inventory(&self) -> &'static Inventory {
        if self.harness == "smoke" {
            &catalog().smoke
        } else {
            &catalog().comparison
        }
    }

    /// Includes matching products, then removes matching exclusions.
    pub fn selects(&self, ty: &str, distribution: &str, len: usize, mode: &str) -> bool {
        self.include
            .iter()
            .any(|rule| rule.matches(ty, distribution, len, mode))
            && !self
                .exclude
                .iter()
                .any(|rule| rule.matches(ty, distribution, len, mode))
    }

    /// Expands products into unique inputs before allocating any records.
    ///
    /// Omitted axes use the harness catalog. An empty experiment returns an error.
    pub fn inputs(&self) -> Result<Vec<InputCase>, String> {
        let inventory = self.inventory();
        let mut cases = BTreeSet::new();
        for rule in &self.include {
            let types = rule
                .types
                .clone()
                .unwrap_or_else(|| inventory.types.clone());
            let shapes = rule
                .distributions
                .clone()
                .unwrap_or_else(|| inventory.shapes.clone());
            let lengths = rule.lengths.clone().unwrap_or_else(|| {
                inventory
                    .lengths
                    .iter()
                    .map(|&len| Length::Count(len))
                    .collect()
            });
            for ty in &types {
                for distribution in &shapes {
                    for length in &lengths {
                        let len = length.resolve(ty);
                        if inventory
                            .modes
                            .iter()
                            .any(|mode| self.selects(ty, distribution, len, mode))
                        {
                            cases.insert(InputCase {
                                ty: ty.clone(),
                                distribution: distribution.clone(),
                                len,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn selection_rules_match_contract() {
        let fixture: Value =
            toml::from_str(include_str!("../../plans/selection.fixture.toml")).unwrap();
        let plan = Plan::parse(&fixture["plan"].to_string(), "smoke").unwrap();
        for input in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                plan.selects(
                    input["type"].as_str().unwrap(),
                    input["distribution"].as_str().unwrap(),
                    input["length"].as_u64().unwrap() as usize,
                    input["mode"].as_str().unwrap()
                ),
                input["selected"].as_bool().unwrap(),
                "{input}"
            );
        }
    }

    #[test]
    fn presets_preserve_case_counts() {
        let continuous = Plan::parse(include_str!("../../plans/continuous.toml"), "smoke").unwrap();
        let cases = continuous.inputs().unwrap();
        assert_eq!(cases.len(), 50);
        assert_eq!(
            cases
                .iter()
                .map(|case| ["api", "workspace"]
                    .iter()
                    .filter(|mode| continuous.selects(&case.ty, &case.distribution, case.len, mode))
                    .count())
                .sum::<usize>(),
            56
        );
        for (text, expected) in [
            (include_str!("../../plans/smoke.toml"), 70),
            (include_str!("../../plans/release.toml"), 260),
        ] {
            assert_eq!(
                Plan::parse(text, "comparison")
                    .unwrap()
                    .inputs()
                    .unwrap()
                    .len(),
                expected
            );
        }
    }

    #[test]
    fn custom_products_expand_once() {
        let text = r#"{"schema":1,"harness":"smoke","algorithms":["tessera"],
            "include":[{"types":["u32","u64"],"distributions":["random","zipf2"],"lengths":[1234]},
                       {"types":["u64"],"distributions":["random"],"lengths":[1234]}],
            "exclude":[{"types":["u32"],"distributions":["zipf2"]}]}"#;
        assert_eq!(
            Plan::parse(text, "smoke").unwrap().inputs().unwrap().len(),
            3
        );
    }

    #[test]
    fn formats_share_selection_rules() {
        let json = r#"{"schema":1,"harness":"smoke","algorithms":["tessera"],
            "include":[{"types":["u64"],"distributions":["random"],"lengths":[1000,"tile-next"]}],
            "exclude":[{"lengths":[1000],"modes":["workspace"]}]}"#;
        let text = r#"
            # Numeric counts and code-relative boundaries share one axis.
            schema = 1
            harness = "smoke"
            algorithms = ["tessera"]
            [[include]]
            types = ["u64"]
            distributions = ["random"]
            lengths = [1_000, "tile-next"]
            [[exclude]]
            lengths = [1_000]
            modes = ["workspace"]
        "#;
        let expected = Plan::parse(json, "smoke").unwrap();
        let actual = Plan::parse(text, "smoke").unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(actual.inputs().unwrap(), expected.inputs().unwrap());
        assert!(!actual.selects("u64", "random", 1000, "workspace"));
        for invalid in [
            text.replace("schema = 1", "schema = 2"),
            text.replace("types =", "type ="),
            text.replace("[1_000,", "[0,"),
            text.replace("[1_000,", "[1_000, 1_000,"),
        ] {
            assert!(Plan::parse(&invalid, "smoke").is_err());
        }
    }

    #[test]
    fn relative_lengths_follow_records() {
        let plan = Plan::parse(r#"{"schema":1,"harness":"smoke","algorithms":["tessera"],
            "include":[{"types":["u64","row_u64"],"distributions":["random"],"lengths":["tile-next"]}]}"#, "smoke").unwrap();
        for case in plan.inputs().unwrap() {
            let bytes = if case.ty == "u64" { 8 } else { 16 };
            assert_eq!(case.len, TILE_BYTES / bytes + 1);
        }
    }
}

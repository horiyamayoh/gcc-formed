use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub(crate) struct RepairOracleOptions {
    pub(crate) root: PathBuf,
    pub(crate) fixture: Option<String>,
    pub(crate) check: bool,
}

#[derive(Debug, Deserialize)]
struct OracleSpec {
    schema_version: u32,
    fixture_id: String,
    compiler: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    defects: Vec<DefectSpec>,
}

#[derive(Debug, Deserialize)]
struct DefectSpec {
    defect_id: String,
    patch: String,
    #[serde(default = "yes")]
    independently_applicable: bool,
    #[serde(default)]
    interaction_group: Option<String>,
    #[serde(default = "yes")]
    observable: bool,
    #[serde(default)]
    primary_repair_anchors: Vec<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct RunEvidence {
    exit_status: i32,
    command: Vec<String>,
    diagnostic_fingerprints: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct DefectEvidence {
    defect_id: String,
    independently_applicable: bool,
    interaction_group: Option<String>,
    observable: bool,
    primary_repair_anchors: Vec<String>,
    repair_run: RunEvidence,
    disappeared_fingerprints: Vec<String>,
    appeared_fingerprints: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct CausalMap {
    schema_version: u32,
    fixture_id: String,
    baseline: RunEvidence,
    defects: Vec<DefectEvidence>,
    fully_repaired: RunEvidence,
    independent_patch_order_stable: bool,
    baseline_comparison: BaselineComparison,
    ambiguity: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct BaselineComparison {
    raw_gcc_diagnostic_count: usize,
    current_formed_visible_count: Option<usize>,
    oracle_repair_unit_count: usize,
}

pub(crate) fn run_repair_oracle(
    options: RepairOracleOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let specs = discover_specs(&options.root)?;
    let mut selected = 0usize;
    for path in specs {
        let spec: OracleSpec = toml::from_str(&fs::read_to_string(&path)?)?;
        if options
            .fixture
            .as_deref()
            .is_some_and(|wanted| wanted != spec.fixture_id)
        {
            continue;
        }
        selected += 1;
        let report = evaluate(path.parent().unwrap(), &spec)?;
        let output = path.parent().unwrap().join("causal-map.json");
        let bytes = canonical_json(&report)?;
        if options.check {
            let expected = fs::read(&output)
                .map_err(|_| format!("missing oracle output {}", output.display()))?;
            if expected != bytes {
                return Err(format!("repair oracle drift for {}", spec.fixture_id).into());
            }
        } else {
            fs::write(&output, bytes)?;
        }
    }
    if selected == 0 {
        return Err("no matching repair-oracle fixtures".into());
    }
    println!("repair oracle verified fixtures: {selected}");
    Ok(())
}

fn discover_specs(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path
                .file_name()
                .is_some_and(|name| name == "repair-oracle.toml")
            {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    walk(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn evaluate(root: &Path, spec: &OracleSpec) -> Result<CausalMap, Box<dyn std::error::Error>> {
    if spec.schema_version != 1 {
        return Err("unsupported repair oracle schema".into());
    }
    if spec.defects.is_empty() {
        return Err("repair oracle requires at least one defect".into());
    }
    let baseline = run_in_copy(root, spec, &[])?;
    let raw_gcc_diagnostic_count = baseline.diagnostic_fingerprints.len();
    let current_formed_visible_count = run_formed_count_in_copy(root, spec)?;
    let baseline_set = baseline
        .diagnostic_fingerprints
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut defects = Vec::new();
    let mut ambiguity = Vec::new();
    for defect in &spec.defects {
        if !defect.independently_applicable && defect.interaction_group.is_none() {
            return Err(format!(
                "non-independent defect {} requires interaction_group",
                defect.defect_id
            )
            .into());
        }
        let repaired = run_in_copy(root, spec, &[defect])?;
        let repaired_set = repaired
            .diagnostic_fingerprints
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let disappeared = baseline_set
            .difference(&repaired_set)
            .cloned()
            .collect::<Vec<_>>();
        let appeared = repaired_set
            .difference(&baseline_set)
            .cloned()
            .collect::<Vec<_>>();
        if defect.observable && disappeared.is_empty() {
            ambiguity.push(defect.defect_id.clone());
        }
        defects.push(DefectEvidence {
            defect_id: defect.defect_id.clone(),
            independently_applicable: defect.independently_applicable,
            interaction_group: defect.interaction_group.clone(),
            observable: defect.observable,
            primary_repair_anchors: defect.primary_repair_anchors.clone(),
            repair_run: repaired,
            disappeared_fingerprints: disappeared,
            appeared_fingerprints: appeared,
        });
    }
    let fully_repaired = run_in_copy(root, spec, &spec.defects.iter().collect::<Vec<_>>())?;
    let mut reversed = spec.defects.iter().collect::<Vec<_>>();
    reversed.reverse();
    let reverse_repaired = run_in_copy(root, spec, &reversed)?;
    let independent_patch_order_stable = spec
        .defects
        .iter()
        .any(|defect| !defect.independently_applicable)
        || fully_repaired == reverse_repaired;
    if !independent_patch_order_stable {
        return Err(format!(
            "independent patch order changed result for {}",
            spec.fixture_id
        )
        .into());
    }
    Ok(CausalMap {
        schema_version: 1,
        fixture_id: spec.fixture_id.clone(),
        baseline,
        defects,
        fully_repaired,
        independent_patch_order_stable,
        baseline_comparison: BaselineComparison {
            raw_gcc_diagnostic_count,
            current_formed_visible_count,
            oracle_repair_unit_count: spec
                .defects
                .iter()
                .filter(|defect| defect.observable)
                .count(),
        },
        ambiguity,
    })
}

fn run_in_copy(
    root: &Path,
    spec: &OracleSpec,
    repairs: &[&DefectSpec],
) -> Result<RunEvidence, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    copy_tree(root, temp.path())?;
    for defect in repairs {
        let status = Command::new("patch")
            .args(["-p1", "--forward", "--batch", "-i"])
            .arg(temp.path().join(&defect.patch))
            .current_dir(temp.path())
            .status()?;
        if !status.success() {
            return Err(format!("failed to apply repair {}", defect.defect_id).into());
        }
    }
    let output = Command::new(&spec.compiler)
        .args(&spec.args)
        .current_dir(temp.path())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let fingerprints = diagnostic_fingerprints(&stderr);
    Ok(RunEvidence {
        exit_status: output.status.code().unwrap_or(128),
        command: std::iter::once(spec.compiler.clone())
            .chain(spec.args.clone())
            .collect(),
        diagnostic_fingerprints: fingerprints,
    })
}

fn run_formed_count_in_copy(
    root: &Path,
    spec: &OracleSpec,
) -> Result<Option<usize>, Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let Some(directory) = executable.parent() else {
        return Ok(None);
    };
    let wrapper = directory.join("gcc-formed");
    if !wrapper.is_file() {
        return Ok(None);
    }
    let backend = Command::new("sh")
        .args(["-c", &format!("command -v {}", spec.compiler)])
        .output()?;
    if !backend.status.success() {
        return Ok(None);
    }
    let backend = String::from_utf8(backend.stdout)?.trim().to_string();
    let temp = tempfile::tempdir()?;
    copy_tree(root, temp.path())?;
    let output = Command::new(wrapper)
        .args(&spec.args)
        .env("FORMED_BACKEND_GCC", backend)
        .current_dir(temp.path())
        .output()?;
    let visible = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.starts_with("error:") || line.contains(": error:"))
        .count();
    Ok(Some(visible))
}

fn diagnostic_fingerprints(stderr: &str) -> Vec<String> {
    let mut values = stderr
        .lines()
        .filter(|line| {
            line.contains(" error:") || line.contains(": error:") || line.contains(" warning:")
        })
        .map(|line| {
            let normalized = line.splitn(4, ':').skip(3).next().unwrap_or(line).trim();
            format!("{:x}", Sha256::digest(normalized.as_bytes()))
        })
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&destination)?;
            copy_tree(&path, &destination)?;
        } else if entry.file_name() != "causal-map.json" {
            fs::copy(path, destination)?;
        }
    }
    Ok(())
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[allow(dead_code)]
pub(crate) fn cascade_compatibility_spec(
    fixture_id: &str,
    expected_roots: Option<u32>,
) -> serde_json::Value {
    serde_json::json!({"fixture_id": fixture_id, "oracle_repair_unit_count": expected_roots, "source": "cascade_expectations_compatibility"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprints_ignore_location_wording_and_are_stable() {
        let a = diagnostic_fingerprints("a.c:1:2: error: missing ;\n");
        let b = diagnostic_fingerprints("b.c:9:8: error: missing ;\n");
        assert_eq!(a, b);
    }
    #[test]
    fn cascade_loader_projects_root_count_without_claiming_causality() {
        assert_eq!(
            cascade_compatibility_spec("x", Some(2))["oracle_repair_unit_count"],
            2
        );
    }

    #[test]
    fn interaction_schema_requires_an_explicit_group() {
        let spec: OracleSpec = toml::from_str(
            r#"schema_version=1
fixture_id="interaction"
compiler="false"
[[defects]]
defect_id="paired"
patch="paired.patch"
independently_applicable=false
interaction_group="pair-a"
"#,
        )
        .unwrap();
        assert_eq!(spec.defects[0].interaction_group.as_deref(), Some("pair-a"));
    }
}

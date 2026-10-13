//! Pack 3: owned static-import collection and finite process-tree RSS workloads.
//! Neither an architecture opinion nor a printed success line is an oracle.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thruline_policy::acceptance::{ArchitectureCheck, Check, Outcome, PackPlan};

pub fn driver(check: &Check) -> Option<&'static str> {
    match crate::verification_checks::plan(check) {
        Some(PackPlan::Architecture { .. }) => Some(include_str!("verification_architecture.py")),
        Some(PackPlan::Resource { .. }) => Some(include_str!("verification_memory.py")),
        _ => None,
    }
}
fn unknown(reason: impl Into<String>) -> (Outcome, String) {
    (Outcome::Unparsed, reason.into())
}
pub fn result(check: &Check, value: &Value) -> (Outcome, String) {
    let plan = crate::verification_checks::plan(check);
    if value["plan"] != serde_json::to_value(plan).unwrap_or(Value::Null) {
        return unknown("The collector did not capture the accepted rule or workload. Rerun with the supported adapter.");
    }
    match plan {
        Some(PackPlan::Architecture {
            check,
            policy,
            graph,
            ..
        }) => {
            let protocol = if graph.is_some() {
                "thruline-architecture/4"
            } else {
                "thruline-architecture/3"
            };
            if value["protocol"] != protocol {
                return unknown("Static import capture is missing or unsupported.");
            }
            let graph_name = match graph {
                Some(thruline_policy::acceptance::ArchitectureGraph::RustPackages) => "declared Cargo package dependencies (all target, optional, build and dev declarations)",
                _ => "Python static module imports",
            };
            if graph.is_some()
                && value["graph_type"] != serde_json::to_value(graph).unwrap_or(Value::Null)
            {
                return unknown(
                    "The captured dependency graph is not the graph accepted by this policy.",
                );
            }
            let Some(policy) = policy else {
                return unknown("No architecture policy is configured. Accept named layer rules and cycle exemptions before judging this project; an empty policy is not evidence of sound architecture.");
            };
            let issues = value["issues"].as_array();
            if issues.is_none_or(|a| !a.is_empty()) {
                let details = issues
                    .into_iter()
                    .flatten()
                    .take(3)
                    .map(|i| {
                        format!(
                            "{}:{} — {}",
                            i["path"].as_str().unwrap_or("source"),
                            i["line"],
                            i["reason"].as_str().unwrap_or("unresolved input")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                return unknown(format!("The static import graph is incomplete. {details} Supply or resolve those inputs, then rerun. Captured edges remain inspectable."));
            }
            let (Some(modules), Some(edges)) =
                (value["modules"].as_array(), value["edges"].as_array())
            else {
                return unknown("Import graph inventory was not captured.");
            };
            if modules.is_empty() || modules.len() > 512 || edges.len() > 8192 {
                return unknown("Static graph inventory is empty or exceeds its supported bounds.");
            }
            let layer = |module: &str| {
                policy
                    .layers
                    .iter()
                    .filter(|(prefix, _)| {
                        module == prefix.as_str() || module.starts_with(&format!("{prefix}."))
                    })
                    .max_by_key(|(p, _)| p.len())
                    .map(|(_, l)| l)
            };
            match check {
                ArchitectureCheck::Edges => {
                    let mut unmapped = BTreeSet::new();
                    for module in modules {
                        if layer(module["name"].as_str().unwrap_or("")).is_none() {
                            unmapped.insert(module["name"].to_string());
                        }
                    }
                    let mut violations = vec![];
                    for edge in edges {
                        let (Some(source), Some(target)) =
                            (edge["source"].as_str(), edge["target"].as_str())
                        else {
                            return unknown("An import edge has no source or target.");
                        };
                        let (Some(from), Some(to)) = (layer(source), layer(target)) else {
                            unmapped.insert(format!("{source} → {target}"));
                            continue;
                        };
                        for rule in &policy.forbidden {
                            if *from == rule.from && *to == rule.to {
                                violations.push(format!(
                                    "{}:{} imports {} → {} ({} → {}), prohibited by rule {}",
                                    edge["path"].as_str().unwrap_or("source"),
                                    edge["line"],
                                    source,
                                    target,
                                    from,
                                    to,
                                    rule.id
                                ));
                            }
                        }
                    }
                    if !unmapped.is_empty() {
                        return unknown(format!("Policy {} does not assign all examined modules/imports to layers: {}. Complete the mapping; broader architecture remains unknown.",policy.id,unmapped.into_iter().take(8).collect::<Vec<_>>().join(", ")));
                    }
                    if !violations.is_empty() {
                        return (Outcome::Failed,format!("{} prohibited dependency edge(s) violate accepted policy {}. {}. Move this dependency behind an allowed module and rerun. This is a static boundary finding, not a claim of runtime failure.",violations.len(),policy.id,violations.iter().take(4).cloned().collect::<Vec<_>>().join("; ")));
                    }
                    (Outcome::Passed,format!("All {} captured edges comply with policy {} across {} nodes in {}. Only these accepted layer rules were checked; runtime reachability, external dependency internals and general architecture quality are outside this result.",edges.len(),policy.id,modules.len(),graph_name))
                }
                ArchitectureCheck::Cycles => {
                    if !policy.forbid_cycles {
                        return unknown("No prohibition on dependency cycles is configured. The captured graph is advisory; accept an explicit cycle rule first.");
                    }
                    let nodes: BTreeSet<_> =
                        modules.iter().filter_map(|m| m["name"].as_str()).collect();
                    // Index once: repeated full-edge scans made dense receipt
                    // replay needlessly expensive. Components use two graph traversals.
                    let mut adjacency: BTreeMap<&str, Vec<&str>> =
                        nodes.iter().map(|&n| (n, Vec::new())).collect();
                    for edge in edges {
                        if let (Some(source), Some(target)) =
                            (edge["source"].as_str(), edge["target"].as_str())
                        {
                            if nodes.contains(target) {
                                if let Some(targets) = adjacency.get_mut(source) {
                                    targets.push(target);
                                }
                            }
                        }
                    }
                    // Two iterative traversals identify strongly connected
                    // components without recursion or per-node graph closure.
                    let mut reverse: BTreeMap<&str, Vec<&str>> =
                        nodes.iter().map(|&n| (n, Vec::new())).collect();
                    for (&source, targets) in &adjacency {
                        for &target in targets {
                            if let Some(parents) = reverse.get_mut(target) {
                                parents.push(source);
                            }
                        }
                    }
                    let mut visited = BTreeSet::new();
                    let mut order = Vec::new();
                    for &node in &nodes {
                        let mut stack = vec![(node, false)];
                        while let Some((n, finished)) = stack.pop() {
                            if finished {
                                order.push(n);
                            } else if visited.insert(n) {
                                stack.push((n, true));
                                stack.extend(adjacency[n].iter().map(|&n| (n, false)));
                            }
                        }
                    }
                    let mut assigned = BTreeSet::new();
                    let mut violations = Vec::new();
                    for n in order.into_iter().rev() {
                        if assigned.contains(n) {
                            continue;
                        }
                        let mut component = BTreeSet::new();
                        let mut todo = vec![n];
                        while let Some(k) = todo.pop() {
                            if assigned.insert(k) {
                                component.insert(k.to_string());
                                todo.extend(&reverse[k]);
                            }
                        }
                        if component.len() == 1 && !adjacency[n].contains(&n) {
                            continue;
                        }
                        if policy
                            .cycle_exemptions
                            .iter()
                            .any(|e| e.modules == component)
                        {
                            continue;
                        }
                        // The witness is separately checked against the captured directed edges.
                        let witness = value["cycles"].as_array().and_then(|cycles| {
                            cycles.iter().find(|c| {
                                c["modules"].as_array().map(|a| {
                                    a.iter()
                                        .filter_map(Value::as_str)
                                        .map(str::to_string)
                                        .collect::<BTreeSet<_>>()
                                }) == Some(component.clone())
                            })
                        });
                        let Some(path) = witness.and_then(|w| w["path"].as_array()) else {
                            return unknown("A prohibited component was found but its cycle path was not captured; inspect the import graph and rerun.");
                        };
                        if path.len() < 2
                            || path.first() != path.last()
                            || !path.windows(2).all(|p| {
                                edges
                                    .iter()
                                    .any(|e| e["source"] == p[0] && e["target"] == p[1])
                            })
                        {
                            return unknown(
                                "The captured cycle witness is not a closed dependency path.",
                            );
                        }
                        violations.push(
                            path.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(" → "),
                        );
                    }
                    if !violations.is_empty() {
                        return (Outcome::Failed,format!("Policy {} forbids unapproved cycles. Found {}. The receipt lists each source line in the cycle. Remove a dependency or explicitly review a justified component exemption, then rerun; static cycles do not themselves prove a runtime bug.",policy.id,violations.join("; ")));
                    }
                    (Outcome::Passed,format!("No unapproved cycle was found across {} nodes in {} under policy {}. Exact approved component exemptions remain in the receipt; external dependency internals and runtime reachability are outside this graph.",modules.len(),graph_name,policy.id))
                }
            }
        }
        Some(PackPlan::Resource {
            warmup_cycles,
            cycles,
            repetitions,
            sampled_peak_budget_bytes,
            retained_growth_budget_bytes,
            ..
        }) => {
            if value["protocol"] != "thruline-memory/3"
                || value["issues"].as_array().is_none_or(|a| !a.is_empty())
            {
                return unknown(format!("Memory measurement is incomplete: {}. Restore the workload/measurement prerequisites and rerun; this does not establish a resource defect.",value["issues"]));
            }
            let Some(reps) = value["repetitions"]
                .as_array()
                .filter(|a| a.len() == *repetitions as usize)
            else {
                return unknown("Not all scheduled workload repetitions completed.");
            };
            let mut peaks = vec![];
            let mut growths = vec![];
            for rep in reps {
                let (Some(samples), Some(phases)) =
                    (rep["samples"].as_array(), rep["phases"].as_array())
                else {
                    return unknown("Required RSS samples or workload phases are missing.");
                };
                if rep["complete"] != true
                    || samples.len() < (*cycles + *warmup_cycles + 1) as usize
                    || samples.len() > 4096
                    || phases.len() != (*cycles + *warmup_cycles + 1) as usize
                {
                    return unknown("The phase or sample inventory is incomplete; no bounded-memory conclusion is available.");
                }
                for (i, p) in phases.iter().enumerate() {
                    if p["cycle"].as_i64() != Some(i as i64 - 1) {
                        return unknown("Workload phase order is incomplete.");
                    }
                }
                for s in samples.iter().chain(phases) {
                    let Some(processes) = s["processes"]
                        .as_array()
                        .filter(|p| !p.is_empty() && p.len() <= 128)
                    else {
                        return unknown("The measured process inventory is incomplete.");
                    };
                    let sum = processes
                        .iter()
                        .try_fold(0u64, |n, p| n.checked_add(p["rss_bytes"].as_u64()?));
                    if sum.is_none() || sum != s["rss_bytes"].as_u64() {
                        return unknown("The process RSS accounting cannot be reproduced.");
                    }
                }
                let peak = samples
                    .iter()
                    .filter_map(|s| s["rss_bytes"].as_u64())
                    .max()
                    .unwrap_or(0);
                let baseline = phases[*warmup_cycles as usize]["rss_bytes"]
                    .as_u64()
                    .unwrap_or(0);
                let growth = phases
                    .iter()
                    .skip(*warmup_cycles as usize + 1)
                    .filter_map(|p| p["rss_bytes"].as_u64())
                    .map(|rss| rss.saturating_sub(baseline))
                    .max()
                    .unwrap_or(0);
                peaks.push(peak);
                growths.push(growth);
            }
            let peak_fail = peaks
                .iter()
                .filter(|p| **p > *sampled_peak_budget_bytes)
                .count();
            let growth_fail = growths
                .iter()
                .filter(|g| **g > *retained_growth_budget_bytes)
                .count();
            let detail=format!("Highest sampled process-tree RSS by repetition: {peaks:?} bytes (budget {sampled_peak_budget_bytes}); greatest post-cycle increase over the last warmup: {growths:?} bytes (budget {retained_growth_budget_bytes}).");
            if peak_fail >= 2 || growth_fail >= 2 {
                return (Outcome::Failed,format!("The scheduled workload exceeded an accepted RSS budget in at least two process repetitions. {detail} Reduce the demonstrated peak or retained allocation and rerun. This is a finite budget violation, not by itself proof of a leak."));
            }
            if peak_fail > 0 || growth_fail > 0 {
                return unknown(format!("A budget breach appeared in only one repetition. {detail} Repeat the same explicitly scheduled workload to establish whether the breach is reproducible."));
            }
            (Outcome::Passed,format!("The declared workload met both sampled peak and post-cycle RSS budgets in all {repetitions} repetitions. {detail} This supports only this workload, sampling method and host; between-sample peaks and indefinite memory growth remain unknown. No RAM ceiling was enforced."))
        }
        _ => unknown("Unsupported structural verification plan."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn maximum_supported_dense_graph_has_a_replayable_cycle() {
        let plan = json!({"kind":"architecture","root":"src","check":"cycles","policy":{"id":"dense","layers":{"m":"layer"},"external_modules":[],"forbidden":[],"forbid_cycles":true,"cycle_exemptions":[]}});
        let check:Check=serde_json::from_value(json!({"id":"cycles","category":"lint","argv":["python3"],"cwd":".","runner":"thruline-python-architecture-v3","mode":"captured","timeout_seconds":10,"artifacts":[],"verification":{"scope":["src"],"environment_files":[],"environment_closed":true,"environment_description":"Synthetic maximum supported static graph","cases":[],"pack":plan}})).unwrap();
        let names: Vec<_> = (0..512).map(|n| format!("m{n}")).collect();
        let mut edges = vec![];
        for i in 0..512 {
            for offset in 1..=16 {
                edges.push(json!({"source":names[i],"target":names[(i+offset)%512],"path":format!("src/{}.py",names[i]),"line":offset}));
            }
        }
        let mut path = names.clone();
        path.push(names[0].clone());
        let value = json!({"protocol":"thruline-architecture/3","plan":plan,"modules":names.iter().map(|n|json!({"name":n})).collect::<Vec<_>>(),"edges":edges,"cycles":[{"modules":names,"path":path}],"issues":[]});
        let started = std::time::Instant::now();
        assert_eq!(result(&check, &value).0, Outcome::Failed);
        eprintln!(
            "Synthetic dense graph: 512 modules, 8192 edges, {} ms replay",
            started.elapsed().as_millis()
        );
    }
}

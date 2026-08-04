use cwl_core::{
    OneOrMany,
    documents::{Workflow, WorkflowStep},
};
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) fn build_execution_tree(workflow: &Workflow) -> crate::Result<Vec<Vec<&WorkflowStep>>> {
    let step_map = workflow
        .steps
        .iter()
        .filter_map(|s| s.id.as_deref().map(|i| (i, s)))
        .collect::<HashMap<_, _>>();

    let step_ids: Vec<&str> = step_map.keys().copied().collect();

    //for each step, collect the set of steps it depends on
    let deps = step_map
        .iter()
        .map(|(&id, &step)| {
            let upstream = collect_step_dependencies(step, &step_map);
            (id, upstream)
        })
        .collect::<HashMap<_, _>>();

    //reverse dependencies, map step_id to the set of steps that depend on it
    let mut rdeps = step_ids
        .iter()
        .map(|&id| (id, HashSet::new()))
        .collect::<HashMap<_, _>>();

    for (&step_id, upstream) in &deps {
        for &dep in upstream {
            rdeps.entry(dep).or_default().insert(step_id);
        }
    }

    // Kahn's algo for toposort (Claude)
    let mut in_degree: HashMap<&str, usize> = deps
        .iter()
        .map(|(&id, upstream)| (id, upstream.len()))
        .collect();

    let mut waves: Vec<Vec<&WorkflowStep>> = Vec::new();
    let mut ready: VecDeque<&str> = in_degree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut visited = 0;

    while !ready.is_empty() {
        // All currently ready steps form one parallel wave
        let wave_ids: Vec<&str> = ready.drain(..).collect();
        let wave_steps: Vec<&WorkflowStep> = wave_ids
            .iter()
            .filter_map(|id| step_map.get(id).copied())
            .collect();
        waves.push(wave_steps);
        visited += wave_ids.len();

        // Decrement in-degree for all downstream steps
        for id in &wave_ids {
            if let Some(downstream) = rdeps.get(id) {
                for &dep in downstream {
                    let deg = in_degree.entry(dep).or_default();
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push_back(dep);
                    }
                }
            }
        }
    }

    if visited != step_map.len() {
        crate::bail!("Cycle detected in workflow step dependencies");
    }

    Ok(waves)
}

fn collect_step_dependencies<'a>(
    step: &'a WorkflowStep,
    step_map: &HashMap<&str, &WorkflowStep>,
) -> HashSet<&'a str> {
    let mut deps = HashSet::new();

    for input in &step.r#in {
        let sources = match &input.source {
            Some(OneOrMany::One(s)) => vec![s.as_str()],
            Some(OneOrMany::Many(ss)) => ss.iter().map(String::as_str).collect(),
            None => vec![],
        };

        for src in sources {
            if let Some((step_id, _output)) = src.split_once('/') {
                let step_id = strip_prefix(step_id);
                if step_map.contains_key(step_id) {
                    deps.insert(step_id);
                }
            }
        }
    }

    deps
}

fn strip_prefix(s: &str) -> &str {
    // Handle "#step_id" or "workflow.cwl#step_id"
    if let Some(pos) = s.rfind('#') {
        &s[pos + 1..]
    } else {
        s
    }
}

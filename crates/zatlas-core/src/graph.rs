use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use ts_rs::TS;

use crate::model::{Edge, EdgeKind, FileId, Graph, ModuleId};

pub struct Adjacency {
    out: Vec<Vec<FileId>>,
    inc: Vec<Vec<FileId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Dependency,

    Structural,
}

impl Adjacency {
    pub fn build(node_count: usize, edges: &[Edge]) -> Self {
        Self::build_for(node_count, edges, Relation::Dependency)
    }

    pub fn build_for(node_count: usize, edges: &[Edge], relation: Relation) -> Self {
        let mut out = vec![Vec::new(); node_count];
        let mut inc = vec![Vec::new(); node_count];
        for e in edges {
            if e.kind == EdgeKind::CoChange {
                continue;
            }
            if e.kind == EdgeKind::Contains && relation == Relation::Dependency {
                continue;
            }
            let (f, t) = (e.from.0 as usize, e.to.0 as usize);
            if f >= node_count || t >= node_count {
                continue;
            }
            out[f].push(e.to);
            inc[t].push(e.from);
        }
        for v in out.iter_mut().chain(inc.iter_mut()) {
            v.sort_unstable_by_key(|id| id.0);
            v.dedup();
        }
        Self { out, inc }
    }

    pub fn dependencies(&self, id: FileId) -> &[FileId] {
        self.out.get(id.0 as usize).map_or(&[], |v| v.as_slice())
    }

    pub fn dependents(&self, id: FileId) -> &[FileId] {
        self.inc.get(id.0 as usize).map_or(&[], |v| v.as_slice())
    }

    pub fn fan_out(&self, id: FileId) -> usize {
        self.dependencies(id).len()
    }

    pub fn fan_in(&self, id: FileId) -> usize {
        self.dependents(id).len()
    }

    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Tangle {
    pub members: Vec<FileId>,

    pub example: Vec<FileId>,
}

pub fn tangles(adj: &Adjacency) -> Vec<Tangle> {
    let n = adj.len();
    let mut index_of = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0u32;
    let mut components: Vec<Vec<FileId>> = Vec::new();

    let mut call: Vec<(usize, usize)> = Vec::new();

    for start in 0..n {
        if index_of[start] != u32::MAX {
            continue;
        }
        call.push((start, 0));

        while let Some(&mut (v, ref mut child)) = call.last_mut() {
            if *child == 0 {
                index_of[v] = next_index;
                low[v] = next_index;
                next_index += 1;
                stack.push(v);
                on_stack[v] = true;
            }

            let neighbours = adj.dependencies(FileId(v as u32));
            if *child < neighbours.len() {
                let w = neighbours[*child].0 as usize;
                *child += 1;
                if index_of[w] == u32::MAX {
                    call.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index_of[w]);
                }
                continue;
            }

            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[v]);
            }

            if low[v] == index_of[v] {
                let mut component = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    component.push(FileId(w as u32));
                    if w == v {
                        break;
                    }
                }
                if component.len() > 1 {
                    component.sort_unstable_by_key(|f| f.0);
                    components.push(component);
                }
            }
        }
    }

    components.sort_by_key(|c| c[0].0);

    components
        .into_iter()
        .map(|members| {
            let example = shortest_cycle_within(adj, &members);
            Tangle { members, example }
        })
        .collect()
}

fn shortest_cycle_within(adj: &Adjacency, members: &[FileId]) -> Vec<FileId> {
    if members.is_empty() {
        return Vec::new();
    }
    let inside: HashSet<FileId> = members.iter().copied().collect();
    let start = members[0];

    let mut prev: HashMap<FileId, FileId> = HashMap::new();
    let mut queue = VecDeque::new();
    for &next in adj.dependencies(start) {
        if next == start {
            return vec![start, start];
        }
        if inside.contains(&next) && !prev.contains_key(&next) {
            prev.insert(next, start);
            queue.push_back(next);
        }
    }

    while let Some(node) = queue.pop_front() {
        for &next in adj.dependencies(node) {
            if next == start {
                let mut path = Vec::new();
                let mut cur = node;
                loop {
                    path.push(cur);
                    if cur == start {
                        break;
                    }
                    match prev.get(&cur) {
                        Some(&p) => cur = p,
                        None => break,
                    }
                }
                path.reverse();
                path.push(start);
                return path;
            }
            if !inside.contains(&next) || prev.contains_key(&next) {
                continue;
            }
            prev.insert(next, node);
            queue.push_back(next);
        }
    }
    members.to_vec()
}

pub fn impact(adj: &Adjacency, seeds: &[FileId]) -> Vec<FileId> {
    reachable(seeds, |id| adj.dependents(id))
}

pub fn dependencies_of(adj: &Adjacency, seeds: &[FileId]) -> Vec<FileId> {
    reachable(seeds, |id| adj.dependencies(id))
}

fn reachable<'a>(seeds: &[FileId], step: impl Fn(FileId) -> &'a [FileId]) -> Vec<FileId> {
    let mut seen: HashSet<FileId> = HashSet::new();
    let mut queue: VecDeque<FileId> = seeds.iter().copied().collect();
    let seed_set: HashSet<FileId> = seeds.iter().copied().collect();

    while let Some(node) = queue.pop_front() {
        for &next in step(node) {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    let mut out: Vec<FileId> = seen.difference(&seed_set).copied().collect();
    out.sort_unstable_by_key(|f| f.0);
    out
}

pub fn orphans(adj: &Adjacency, entrypoints: &[FileId]) -> Vec<FileId> {
    let reachable_from_entries: HashSet<FileId> = {
        let mut seen: HashSet<FileId> = entrypoints.iter().copied().collect();
        let mut queue: VecDeque<FileId> = entrypoints.iter().copied().collect();
        while let Some(node) = queue.pop_front() {
            for &next in adj.dependencies(node) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        seen
    };

    (0..adj.len())
        .map(|i| FileId(i as u32))
        .filter(|&id| adj.fan_in(id) == 0 && !reachable_from_entries.contains(&id))
        .collect()
}

pub fn module_edges(graph: &Graph) -> Vec<(ModuleId, ModuleId, u32)> {
    let mut counts: HashMap<(u32, u32), u32> = HashMap::new();
    for e in &graph.edges {
        if e.kind == EdgeKind::CoChange {
            continue;
        }
        let (Some(f), Some(t)) = (graph.file(e.from), graph.file(e.to)) else {
            continue;
        };
        if f.module == t.module {
            continue;
        }
        *counts.entry((f.module.0, t.module.0)).or_insert(0) += 1;
    }
    let mut out: Vec<(ModuleId, ModuleId, u32)> = counts
        .into_iter()
        .map(|((f, t), n)| (ModuleId(f), ModuleId(t), n))
        .collect();
    out.sort_by_key(|(f, t, _)| (f.0, t.0));
    out
}

pub fn dsm_order(adj: &Adjacency) -> Vec<FileId> {
    let n = adj.len();
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);

    let mut roots: Vec<usize> = (0..n).collect();
    roots.sort_by_key(|&i| (adj.fan_in(FileId(i as u32)), i));

    for start in roots {
        if visited[start] {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        visited[start] = true;
        while let Some(&mut (v, ref mut child)) = stack.last_mut() {
            let deps = adj.dependencies(FileId(v as u32));
            if *child < deps.len() {
                let w = deps[*child].0 as usize;
                *child += 1;
                if !visited[w] {
                    visited[w] = true;
                    stack.push((w, 0));
                }
                continue;
            }
            stack.pop();
            order.push(FileId(v as u32));
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: u32, to: u32) -> Edge {
        Edge {
            from: FileId(from),
            to: FileId(to),
            kind: EdgeKind::Import,
            weight: 1.0,
        }
    }

    fn adj(n: usize, pairs: &[(u32, u32)]) -> Adjacency {
        let edges: Vec<Edge> = pairs.iter().map(|&(f, t)| edge(f, t)).collect();
        Adjacency::build(n, &edges)
    }

    fn ids(v: &[FileId]) -> Vec<u32> {
        v.iter().map(|f| f.0).collect()
    }

    #[test]
    fn adjacency_records_both_directions() {
        let a = adj(3, &[(0, 1), (0, 2), (1, 2)]);
        assert_eq!(ids(a.dependencies(FileId(0))), [1, 2]);
        assert_eq!(ids(a.dependents(FileId(2))), [0, 1]);
        assert_eq!(a.fan_in(FileId(2)), 2);
        assert_eq!(a.fan_out(FileId(0)), 2);
    }

    #[test]
    fn co_change_edges_are_not_dependencies() {
        let edges = vec![
            edge(0, 1),
            Edge {
                from: FileId(1),
                to: FileId(0),
                kind: EdgeKind::CoChange,
                weight: 0.9,
            },
        ];
        let a = Adjacency::build(2, &edges);
        assert_eq!(ids(a.dependencies(FileId(1))), [] as [u32; 0]);
        assert!(tangles(&a).is_empty(), "co-change must not create a cycle");
    }

    #[test]
    fn containment_is_not_a_dependency_but_is_structural() {
        let edges = vec![
            Edge {
                from: FileId(0),
                to: FileId(1),
                kind: EdgeKind::Contains,
                weight: 1.0,
            },
            edge(1, 0),
        ];
        let dep = Adjacency::build_for(2, &edges, Relation::Dependency);
        assert!(
            tangles(&dep).is_empty(),
            "mod + use crate:: is idiomatic, not a tangle"
        );

        let structural = Adjacency::build_for(2, &edges, Relation::Structural);
        assert_eq!(ids(structural.dependencies(FileId(0))), [1]);

        assert!(orphans(&structural, &[FileId(0)]).is_empty());
    }

    #[test]
    fn an_acyclic_graph_has_no_tangles() {
        let a = adj(4, &[(0, 1), (1, 2), (2, 3)]);
        assert!(tangles(&a).is_empty());
    }

    #[test]
    fn a_two_node_cycle_is_found_with_its_chain() {
        let a = adj(2, &[(0, 1), (1, 0)]);
        let t = tangles(&a);
        assert_eq!(t.len(), 1);
        assert_eq!(ids(&t[0].members), [0, 1]);
        assert_eq!(ids(&t[0].example), [0, 1, 0]);
    }

    #[test]
    fn a_three_node_cycle_is_found_in_order() {
        let a = adj(3, &[(0, 1), (1, 2), (2, 0)]);
        let t = tangles(&a);
        assert_eq!(t.len(), 1);
        assert_eq!(ids(&t[0].members), [0, 1, 2]);
        assert_eq!(ids(&t[0].example), [0, 1, 2, 0]);
    }

    #[test]
    fn a_chain_hanging_off_a_cycle_is_not_part_of_it() {
        let a = adj(4, &[(0, 1), (1, 0), (2, 0), (3, 2)]);
        let t = tangles(&a);
        assert_eq!(t.len(), 1);
        assert_eq!(ids(&t[0].members), [0, 1]);
    }

    #[test]
    fn two_separate_cycles_are_reported_separately() {
        let a = adj(4, &[(0, 1), (1, 0), (2, 3), (3, 2)]);
        let t = tangles(&a);
        assert_eq!(t.len(), 2);
        assert_eq!(ids(&t[0].members), [0, 1]);
        assert_eq!(ids(&t[1].members), [2, 3]);
    }

    #[test]
    fn the_example_chain_is_a_shortest_cycle_not_the_whole_component() {
        let a = adj(6, &[(0, 1), (1, 0), (1, 2), (2, 3), (3, 4), (4, 5), (5, 1)]);
        let t = tangles(&a);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].members.len(), 6);
        assert_eq!(ids(&t[0].example), [0, 1, 0]);
    }

    #[test]
    fn tarjan_survives_a_very_long_chain_without_overflowing_the_stack() {
        let n = 60_000usize;
        let pairs: Vec<(u32, u32)> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
        let a = adj(n, &pairs);
        assert!(tangles(&a).is_empty());
    }

    #[test]
    fn a_very_long_cycle_is_still_found() {
        let n = 10_000usize;
        let mut pairs: Vec<(u32, u32)> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
        pairs.push((n as u32 - 1, 0));
        let a = adj(n, &pairs);
        let t = tangles(&a);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].members.len(), n);
    }

    #[test]
    fn impact_finds_everything_that_would_break() {
        let a = adj(4, &[(1, 0), (2, 1), (3, 2)]);
        assert_eq!(ids(&impact(&a, &[FileId(0)])), [1, 2, 3]);
        assert_eq!(ids(&impact(&a, &[FileId(2)])), [3]);
        assert_eq!(ids(&impact(&a, &[FileId(3)])), [] as [u32; 0]);
    }

    #[test]
    fn impact_terminates_on_a_cycle() {
        let a = adj(3, &[(0, 1), (1, 0), (2, 0)]);
        let got = impact(&a, &[FileId(0)]);
        assert_eq!(ids(&got), [1, 2]);
    }

    #[test]
    fn dependencies_of_walks_the_other_direction() {
        let a = adj(4, &[(3, 2), (2, 1), (1, 0)]);
        assert_eq!(ids(&dependencies_of(&a, &[FileId(3)])), [0, 1, 2]);
    }

    #[test]
    fn orphans_exclude_anything_an_entry_point_reaches() {
        let a = adj(3, &[(0, 1)]);
        assert_eq!(ids(&orphans(&a, &[FileId(0)])), [2]);
    }

    #[test]
    fn without_entry_points_every_root_would_look_dead() {
        let a = adj(3, &[(0, 1)]);
        assert_eq!(ids(&orphans(&a, &[])), [0, 2]);
    }

    #[test]
    fn module_edges_roll_up_and_drop_self_loops() {
        use crate::model::{FileNode, Language, ModuleNode};
        let file = |id: u32, module: u32, path: &str| FileNode {
            id: FileId(id),
            path: path.into(),
            module: ModuleId(module),
            language: Language::TypeScript,
            loc: 1,
            bytes: 1,
            content_hash: String::new(),
        };
        let graph = Graph {
            files: vec![
                file(0, 0, "ui/a.ts"),
                file(1, 0, "ui/b.ts"),
                file(2, 1, "data/c.ts"),
            ],
            modules: vec![
                ModuleNode {
                    id: ModuleId(0),
                    path: "ui".into(),
                    files: vec![FileId(0), FileId(1)],
                },
                ModuleNode {
                    id: ModuleId(1),
                    path: "data".into(),
                    files: vec![FileId(2)],
                },
            ],

            edges: vec![edge(0, 1), edge(0, 2), edge(1, 2)],
            ..Default::default()
        };
        assert_eq!(module_edges(&graph), [(ModuleId(0), ModuleId(1), 2)]);
    }

    #[test]
    fn dsm_order_puts_dependencies_before_dependents() {
        let a = adj(3, &[(2, 1), (1, 0)]);
        let order = ids(&dsm_order(&a));
        let pos = |x: u32| order.iter().position(|&y| y == x).unwrap();
        assert!(pos(0) < pos(1), "got {order:?}");
        assert!(pos(1) < pos(2), "got {order:?}");
    }

    #[test]
    fn dsm_order_includes_every_node_exactly_once() {
        let a = adj(6, &[(0, 1), (1, 2), (3, 4), (5, 0)]);
        let mut order = ids(&dsm_order(&a));
        order.sort_unstable();
        assert_eq!(order, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn dsm_order_is_stable_across_runs() {
        let a = adj(8, &[(0, 1), (1, 2), (2, 3), (4, 5), (6, 7), (7, 0)]);
        assert_eq!(dsm_order(&a), dsm_order(&a));
    }

    #[test]
    fn an_edge_pointing_outside_the_node_range_is_ignored_rather_than_panicking() {
        let edges = vec![edge(0, 99)];
        let a = Adjacency::build(2, &edges);
        assert!(a.dependencies(FileId(0)).is_empty());
    }
}

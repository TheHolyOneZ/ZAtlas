use crate::graph::Adjacency;
use crate::model::{EdgeKind, FileId, Graph};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub xy: Vec<f32>,

    pub radius: Vec<f32>,
    pub iterations: u32,

    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Layout {
    pub fn len(&self) -> usize {
        self.xy.len() / 2
    }

    pub fn is_empty(&self) -> bool {
        self.xy.is_empty()
    }

    pub fn position(&self, id: FileId) -> Option<(f32, f32)> {
        let i = id.0 as usize * 2;
        Some((*self.xy.get(i)?, *self.xy.get(i + 1)?))
    }
}

pub struct LayoutOptions {
    pub iterations: u32,

    pub repulsion: f32,

    pub attraction: f32,

    pub cluster_strength: f32,

    pub gravity: f32,

    pub theta: f32,

    pub seed: u64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            iterations: 500,
            repulsion: 2.4,
            attraction: 1.0,

            cluster_strength: 0.02,
            gravity: 0.06,
            theta: 0.8,
            seed: 0x5A71A5,
        }
    }
}

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_signed(&mut self) -> f32 {
        (self.next_u64() >> 11) as f32 / (1u64 << 52) as f32 * 2.0 - 1.0
    }
}

struct Quad {
    cx: f32,
    cy: f32,
    half: f32,
    mass: f32,
    com_x: f32,
    com_y: f32,

    children: [usize; 4],

    body: Option<usize>,
}

struct Tree {
    nodes: Vec<Quad>,
}

const NONE: usize = usize::MAX;

impl Tree {
    fn new(cx: f32, cy: f32, half: f32) -> Self {
        Self {
            nodes: vec![Quad {
                cx,
                cy,
                half,
                mass: 0.0,
                com_x: 0.0,
                com_y: 0.0,
                children: [NONE; 4],
                body: None,
            }],
        }
    }

    fn quadrant(&self, node: usize, x: f32, y: f32) -> usize {
        let q = &self.nodes[node];
        match (x >= q.cx, y >= q.cy) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        }
    }

    fn child_box(&self, node: usize, quadrant: usize) -> (f32, f32, f32) {
        let q = &self.nodes[node];
        let h = q.half * 0.5;
        let dx = if quadrant & 1 == 1 { h } else { -h };
        let dy = if quadrant & 2 == 2 { h } else { -h };
        (q.cx + dx, q.cy + dy, h)
    }

    fn insert(&mut self, node: usize, body: usize, xs: &[f32], ys: &[f32], depth: u32) {
        let (x, y) = (xs[body], ys[body]);

        if depth > 48 {
            self.nodes[node].mass += 1.0;
            self.nodes[node].com_x += x;
            self.nodes[node].com_y += y;
            return;
        }

        let is_leaf = self.nodes[node].children.iter().all(|&c| c == NONE);

        if is_leaf && self.nodes[node].body.is_none() && self.nodes[node].mass == 0.0 {
            self.nodes[node].body = Some(body);
            self.nodes[node].mass = 1.0;
            self.nodes[node].com_x = x;
            self.nodes[node].com_y = y;
            return;
        }

        if let Some(existing) = self.nodes[node].body.take() {
            self.nodes[node].mass = 0.0;
            self.nodes[node].com_x = 0.0;
            self.nodes[node].com_y = 0.0;
            self.push_down(node, existing, xs, ys, depth);
        }

        self.push_down(node, body, xs, ys, depth);
        self.nodes[node].mass += 1.0;
        self.nodes[node].com_x += x;
        self.nodes[node].com_y += y;
    }

    fn push_down(&mut self, node: usize, body: usize, xs: &[f32], ys: &[f32], depth: u32) {
        let q = self.quadrant(node, xs[body], ys[body]);
        let child = self.nodes[node].children[q];
        let child = if child == NONE {
            let (cx, cy, half) = self.child_box(node, q);
            self.nodes.push(Quad {
                cx,
                cy,
                half,
                mass: 0.0,
                com_x: 0.0,
                com_y: 0.0,
                children: [NONE; 4],
                body: None,
            });
            let idx = self.nodes.len() - 1;
            self.nodes[node].children[q] = idx;
            idx
        } else {
            child
        };
        self.insert(child, body, xs, ys, depth + 1);
    }

    fn force_on(&self, body: usize, xs: &[f32], ys: &[f32], theta: f32, k: f32) -> (f32, f32) {
        let (bx, by) = (xs[body], ys[body]);
        let (mut fx, mut fy) = (0.0f32, 0.0f32);
        let mut stack = vec![0usize];

        while let Some(idx) = stack.pop() {
            let q = &self.nodes[idx];
            if q.mass == 0.0 {
                continue;
            }
            if q.body == Some(body) {
                continue;
            }

            let cx = q.com_x / q.mass;
            let cy = q.com_y / q.mass;
            let dx = bx - cx;
            let dy = by - cy;
            let dist_sq = dx * dx + dy * dy + 0.01;
            let dist = dist_sq.sqrt();

            let far_enough = q.half * 2.0 / dist < theta;
            if far_enough || q.body.is_some() {
                let f = k * k * q.mass / dist;
                fx += dx / dist * f;
                fy += dy / dist * f;
                continue;
            }
            for &c in &q.children {
                if c != NONE {
                    stack.push(c);
                }
            }
        }
        (fx, fy)
    }
}

pub fn layout(graph: &Graph, adj: &Adjacency, opts: &LayoutOptions) -> Layout {
    let n = graph.files.len();
    if n == 0 {
        return Layout::default();
    }

    let mut rng = Rng(opts.seed);

    let k = 64.0 * opts.repulsion;
    let spread = k * (n as f32).sqrt() * 0.4;
    let mut xs: Vec<f32> = Vec::with_capacity(n);
    let mut ys: Vec<f32> = Vec::with_capacity(n);
    for _ in 0..n {
        xs.push(rng.next_signed() * spread);
        ys.push(rng.next_signed() * spread);
    }

    let radius: Vec<f32> = graph
        .files
        .iter()
        .map(|f| 3.0 + (f.loc as f32).sqrt() * 0.6)
        .collect();

    let module_of: Vec<usize> = graph.files.iter().map(|f| f.module.0 as usize).collect();
    let module_count = graph.modules.len().max(1);

    let edges: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter(|e| e.kind != EdgeKind::CoChange)
        .map(|e| (e.from.0 as usize, e.to.0 as usize))
        .filter(|&(f, t)| f < n && t < n)
        .collect();

    let mut degree = vec![0u32; n];
    for &(a, b) in &edges {
        degree[a] += 1;
        degree[b] += 1;
    }

    let mut vx = vec![0.0f32; n];
    let mut vy = vec![0.0f32; n];

    for iteration in 0..opts.iterations {
        let alpha = 1.0 - (iteration as f32 / opts.iterations as f32);
        let alpha = alpha * alpha;

        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for i in 0..n {
            min_x = min_x.min(xs[i]);
            min_y = min_y.min(ys[i]);
            max_x = max_x.max(xs[i]);
            max_y = max_y.max(ys[i]);
        }
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        let half = ((max_x - min_x).max(max_y - min_y) * 0.5).max(1.0) * 1.2;

        let mut tree = Tree::new(cx, cy, half);
        for i in 0..n {
            tree.insert(0, i, &xs, &ys, 0);
        }

        let mut fx = vec![0.0f32; n];
        let mut fy = vec![0.0f32; n];

        for i in 0..n {
            let (rx, ry) = tree.force_on(i, &xs, &ys, opts.theta, k);
            fx[i] += rx;
            fy[i] += ry;
        }

        for &(a, b) in &edges {
            let dx = xs[b] - xs[a];
            let dy = ys[b] - ys[a];
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);

            let f = (opts.attraction * dist * dist / k).min(k * 2.0);
            let damp_a = 1.0 / (1.0 + degree[a] as f32 * 0.08);
            let damp_b = 1.0 / (1.0 + degree[b] as f32 * 0.08);
            fx[a] += dx / dist * f * damp_a;
            fy[a] += dy / dist * f * damp_a;
            fx[b] -= dx / dist * f * damp_b;
            fy[b] -= dy / dist * f * damp_b;
        }

        if opts.cluster_strength > 0.0 && module_count > 1 {
            let mut sum_x = vec![0.0f32; module_count];
            let mut sum_y = vec![0.0f32; module_count];
            let mut counts = vec![0u32; module_count];
            for i in 0..n {
                let m = module_of[i].min(module_count - 1);
                sum_x[m] += xs[i];
                sum_y[m] += ys[i];
                counts[m] += 1;
            }
            for i in 0..n {
                let m = module_of[i].min(module_count - 1);
                if counts[m] < 2 {
                    continue;
                }
                let mx = sum_x[m] / counts[m] as f32;
                let my = sum_y[m] / counts[m] as f32;
                fx[i] += (mx - xs[i]) * opts.cluster_strength;
                fy[i] += (my - ys[i]) * opts.cluster_strength;
            }
        }

        if opts.gravity > 0.0 {
            let mut gx = 0.0f32;
            let mut gy = 0.0f32;
            for i in 0..n {
                gx += xs[i];
                gy += ys[i];
            }
            gx /= n as f32;
            gy /= n as f32;
            for i in 0..n {
                fx[i] += (gx - xs[i]) * opts.gravity;
                fy[i] += (gy - ys[i]) * opts.gravity;
            }
        }

        let max_step = k * (0.35 * alpha + 0.008);
        for i in 0..n {
            vx[i] = (vx[i] + fx[i]) * 0.82;
            vy[i] = (vy[i] + fy[i]) * 0.82;
            let speed = (vx[i] * vx[i] + vy[i] * vy[i]).sqrt();
            if speed > max_step {
                vx[i] *= max_step / speed;
                vy[i] *= max_step / speed;
            }
            xs[i] += vx[i];
            ys[i] += vy[i];
        }
    }

    let _ = adj;

    let mut xy = Vec::with_capacity(n * 2);
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for i in 0..n {
        xy.push(xs[i]);
        xy.push(ys[i]);
        min_x = min_x.min(xs[i]);
        min_y = min_y.min(ys[i]);
        max_x = max_x.max(xs[i]);
        max_y = max_y.max(ys[i]);
    }

    Layout {
        xy,
        radius,
        iterations: opts.iterations,
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, FileNode, Language, ModuleId, ModuleNode};

    fn graph_of(n: usize, edges: &[(u32, u32)], modules: usize) -> Graph {
        let files: Vec<FileNode> = (0..n)
            .map(|i| FileNode {
                id: FileId(i as u32),
                path: format!("f{i}.ts"),
                module: ModuleId((i % modules.max(1)) as u32),
                language: Language::TypeScript,
                loc: 100,
                bytes: 1000,
                content_hash: String::new(),
            })
            .collect();
        Graph {
            modules: (0..modules)
                .map(|m| ModuleNode {
                    id: ModuleId(m as u32),
                    path: format!("m{m}"),
                    files: files
                        .iter()
                        .filter(|f| f.module.0 as usize == m)
                        .map(|f| f.id)
                        .collect(),
                })
                .collect(),
            files,
            edges: edges
                .iter()
                .map(|&(f, t)| Edge {
                    from: FileId(f),
                    to: FileId(t),
                    kind: EdgeKind::Import,
                    weight: 1.0,
                })
                .collect(),
            ..Default::default()
        }
    }

    fn run(g: &Graph, opts: &LayoutOptions) -> Layout {
        let adj = Adjacency::build(g.files.len(), &g.edges);
        layout(g, &adj, opts)
    }

    fn quick() -> LayoutOptions {
        LayoutOptions {
            iterations: 60,
            ..Default::default()
        }
    }

    #[test]
    fn an_empty_graph_lays_out_to_nothing() {
        let g = Graph::default();
        let l = run(&g, &quick());
        assert!(l.is_empty());
    }

    #[test]
    fn every_node_gets_a_position() {
        let g = graph_of(20, &[(0, 1), (1, 2)], 3);
        let l = run(&g, &quick());
        assert_eq!(l.len(), 20);
        assert_eq!(l.radius.len(), 20);
        assert!(l.position(FileId(19)).is_some());
    }

    #[test]
    fn the_same_repository_always_draws_the_same_map() {
        let g = graph_of(40, &[(0, 1), (1, 2), (2, 3), (5, 6), (7, 8)], 4);
        let a = run(&g, &quick());
        let b = run(&g, &quick());
        assert_eq!(a.xy, b.xy);
    }

    #[test]
    fn a_different_seed_gives_a_different_map() {
        let g = graph_of(30, &[(0, 1), (1, 2)], 3);
        let a = run(&g, &quick());
        let b = run(
            &g,
            &LayoutOptions {
                seed: 999,
                ..quick()
            },
        );
        assert_ne!(a.xy, b.xy);
    }

    #[test]
    fn positions_are_finite_and_never_nan() {
        let g = graph_of(50, &[(0, 1), (1, 0), (2, 2)], 5);
        let l = run(&g, &LayoutOptions::default());
        for (i, v) in l.xy.iter().enumerate() {
            assert!(v.is_finite(), "component {i} is {v}");
        }
    }

    #[test]
    fn nodes_at_identical_positions_do_not_hang_the_quadtree() {
        let g = graph_of(64, &[], 1);
        let mut opts = quick();
        opts.repulsion = 0.0;
        opts.cluster_strength = 0.0;
        let l = run(&g, &opts);
        assert_eq!(l.len(), 64);
    }

    #[test]
    fn connected_nodes_end_up_closer_than_unconnected_ones() {
        let g = graph_of(30, &[(0, 1)], 1);
        let l = run(&g, &LayoutOptions::default());
        let (x0, y0) = l.position(FileId(0)).unwrap();
        let (x1, y1) = l.position(FileId(1)).unwrap();
        let linked = ((x0 - x1).powi(2) + (y0 - y1).powi(2)).sqrt();

        let mut total = 0.0;
        let mut count = 0;
        for i in 2..30u32 {
            for j in (i + 1)..30 {
                let (ax, ay) = l.position(FileId(i)).unwrap();
                let (bx, by) = l.position(FileId(j)).unwrap();
                total += ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
                count += 1;
            }
        }
        let average = total / count as f32;
        assert!(
            linked < average,
            "linked pair {linked} should be closer than the average {average}"
        );
    }

    #[test]
    fn radius_uses_sqrt_so_one_huge_file_cannot_eat_the_screen() {
        let mut g = graph_of(2, &[], 1);
        g.files[0].loc = 100;
        g.files[1].loc = 10_000;
        let l = run(&g, &quick());
        let ratio = l.radius[1] / l.radius[0];
        assert!(
            ratio < 12.0,
            "a 100x LOC difference became {ratio}x radius; linear scaling was the bug"
        );
    }

    #[test]
    fn the_bounding_box_covers_every_node() {
        let g = graph_of(25, &[(0, 1), (3, 4)], 3);
        let l = run(&g, &quick());
        for i in 0..l.len() {
            let (x, y) = l.position(FileId(i as u32)).unwrap();
            assert!(x >= l.min_x && x <= l.max_x, "x {x} outside box");
            assert!(y >= l.min_y && y <= l.max_y, "y {y} outside box");
        }
    }

    #[test]
    fn positions_are_packed_as_pairs_ready_for_a_float32array() {
        let g = graph_of(5, &[], 1);
        let l = run(&g, &quick());
        assert_eq!(l.xy.len(), 10, "x and y interleaved, nothing else");
    }

    fn crowding(l: &Layout) -> f32 {
        let n = l.len();
        let mut total = 0.0f32;
        for i in 0..n {
            let (x, y) = l.position(FileId(i as u32)).unwrap();
            let mut nearest = f32::MAX;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let (ox, oy) = l.position(FileId(j as u32)).unwrap();
                nearest = nearest.min(((x - ox).powi(2) + (y - oy).powi(2)).sqrt());
            }
            total += nearest / l.radius[i].max(1.0);
        }
        total / n as f32
    }

    #[test]
    fn nodes_do_not_pile_on_top_of_each_other() {
        let g = graph_of(160, &[(0, 1), (1, 2), (2, 3), (10, 11), (20, 21)], 12);
        let l = run(&g, &LayoutOptions::default());
        let ratio = crowding(&l);
        assert!(
            ratio > 3.0,
            "nearest neighbours sit {ratio:.2}x the node radius apart; the graph is a blob"
        );
    }

    #[test]
    fn a_densely_connected_graph_still_separates_its_nodes() {
        let mut pairs = Vec::new();
        for i in 0..60u32 {
            pairs.push((i, (i + 1) % 60));
            pairs.push((i, (i + 7) % 60));
        }
        let g = graph_of(60, &pairs, 6);
        let l = run(&g, &LayoutOptions::default());
        assert!(crowding(&l) > 2.0, "got {}", crowding(&l));
    }

    #[test]
    fn a_disconnected_node_stays_near_the_rest_of_the_graph() {
        let g = graph_of(40, &[(0, 1), (1, 2), (2, 3), (3, 4)], 4);
        let l = run(&g, &LayoutOptions::default());

        let width = l.max_x - l.min_x;
        let height = l.max_y - l.min_y;

        let mut xs: Vec<f32> = (0..l.len())
            .map(|i| l.position(FileId(i as u32)).unwrap().0)
            .collect();
        xs.sort_by(f32::total_cmp);
        let p05 = xs[xs.len() / 20];
        let p95 = xs[xs.len() - 1 - xs.len() / 20];
        let bulk = (p95 - p05).max(1.0);

        assert!(
            width / bulk < 3.0,
            "the extent is {width:.0} but the bulk spans only {bulk:.0}; outliers are running away"
        );
        assert!(height.is_finite());
    }

    #[test]
    fn a_thousand_node_graph_settles_in_reasonable_time() {
        let pairs: Vec<(u32, u32)> = (0..999).map(|i| (i, i + 1)).collect();
        let g = graph_of(1000, &pairs, 20);
        let started = std::time::Instant::now();
        let l = run(&g, &LayoutOptions::default());
        assert_eq!(l.len(), 1000);
        assert!(
            started.elapsed().as_secs() < 20,
            "Barnes-Hut should keep this well under the limit, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn co_change_edges_do_not_pull_the_layout() {
        let mut g = graph_of(10, &[], 1);
        g.edges.push(Edge {
            from: FileId(0),
            to: FileId(9),
            kind: EdgeKind::CoChange,
            weight: 1.0,
        });
        let a = run(&g, &quick());
        let plain = run(&graph_of(10, &[], 1), &quick());
        assert_eq!(a.xy, plain.xy);
    }
}

use petgraph::algo::all_simple_paths;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::HashMap;

use super::package::{PackageId, PackageInfo};

/// 依存関係の有向グラフ
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub root_id: PackageId,
    pub graph: DiGraph<PackageInfo, ()>,
    pub node_indices: HashMap<PackageId, NodeIndex>,
}

impl DependencyGraph {
    pub fn new(root_pkg: PackageInfo) -> Self {
        let mut graph = DiGraph::new();
        let root_id = root_pkg.id.clone();
        let idx = graph.add_node(root_pkg);
        let mut node_indices = HashMap::new();
        node_indices.insert(root_id.clone(), idx);

        Self {
            root_id,
            graph,
            node_indices,
        }
    }

    /// パッケージノードを取得または新規追加（UNKNOWN dummyがある場合は実データで上書き）
    pub fn get_or_add_node(&mut self, pkg: PackageInfo) -> NodeIndex {
        if let Some(&idx) = self.node_indices.get(&pkg.id) {
            if let Some(existing) = self.graph.node_weight_mut(idx) {
                if existing.license.category == crate::model::LicenseCategory::Unknown && pkg.license.category != crate::model::LicenseCategory::Unknown {
                    *existing = pkg;
                }
            }
            idx
        } else {
            let id = pkg.id.clone();
            let idx = self.graph.add_node(pkg);
            self.node_indices.insert(id, idx);
            idx
        }
    }

    /// 依存エッジの追加 (from -> to)
    pub fn add_dependency(&mut self, from: &PackageId, to_pkg: PackageInfo) -> NodeIndex {
        let from_idx = if let Some(&idx) = self.node_indices.get(from) {
            idx
        } else {
            let dummy = PackageInfo::new(
                from.clone(),
                "UNKNOWN",
                crate::model::package::DependencyType::Transitive,
                crate::model::package::DependencyScope::Production,
            );
            self.get_or_add_node(dummy)
        };

        let to_idx = self.get_or_add_node(to_pkg);
        
        // 既存エッジの重複チェック
        if !self.graph.contains_edge(from_idx, to_idx) {
            self.graph.add_edge(from_idx, to_idx, ());
        }

        to_idx
    }

    /// 全パッケージノードの一覧を取得
    pub fn all_packages(&self) -> Vec<&PackageInfo> {
        self.graph.node_weights().collect()
    }

    /// ルートパッケージを取得
    pub fn root_package(&self) -> Option<&PackageInfo> {
        self.node_indices.get(&self.root_id).and_then(|&idx| self.graph.node_weight(idx))
    }

    /// 指定したターゲットパッケージへの依存パス（最大30件まで）を探索
    pub fn find_all_paths_to(&self, target_name: &str) -> Vec<Vec<PackageId>> {
        let root_idx = match self.node_indices.get(&self.root_id) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        let mut target_indices = Vec::new();
        for (id, &idx) in &self.node_indices {
            if id.name == target_name || id.to_string_repr() == target_name {
                target_indices.push(idx);
            }
        }

        let mut results = Vec::new();
        for target_idx in target_indices {
            if root_idx == target_idx {
                results.push(vec![self.root_id.clone()]);
                continue;
            }

            // 最大深度 10、最大 30 パスまでの探索（指数爆発防止）
            let paths = all_simple_paths::<Vec<_>, _>(&self.graph, root_idx, target_idx, 0, Some(10));
            for p in paths {
                let id_path: Vec<PackageId> = p
                    .into_iter()
                    .filter_map(|idx| self.graph.node_weight(idx).map(|info| info.id.clone()))
                    .collect();
                results.push(id_path);
                if results.len() >= 30 {
                    break;
                }
            }
            if results.len() >= 30 {
                break;
            }
        }

        results
    }

    /// BFS による高速な最短パス探索 O(V + E)
    pub fn find_shortest_path_to(&self, target_name: &str) -> Option<Vec<PackageId>> {
        let root_idx = match self.node_indices.get(&self.root_id) {
            Some(&idx) => idx,
            None => return None,
        };

        let mut target_indices: std::collections::HashSet<NodeIndex> = std::collections::HashSet::new();
        for (id, &idx) in &self.node_indices {
            if id.name == target_name || id.to_string_repr() == target_name {
                target_indices.insert(idx);
            }
        }

        if target_indices.is_empty() {
            return None;
        }

        if target_indices.contains(&root_idx) {
            return Some(vec![self.root_id.clone()]);
        }

        // BFS キュー: (現在のノード, 辿ってきたパス)
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();

        queue.push_back((root_idx, vec![self.root_id.clone()]));
        visited.insert(root_idx);

        while let Some((curr_idx, curr_path)) = queue.pop_front() {
            for neighbor_idx in self.graph.neighbors_directed(curr_idx, Direction::Outgoing) {
                if let Some(pkg) = self.graph.node_weight(neighbor_idx) {
                    let mut next_path = curr_path.clone();
                    next_path.push(pkg.id.clone());

                    if target_indices.contains(&neighbor_idx) {
                        return Some(next_path);
                    }

                    if visited.insert(neighbor_idx) {
                        queue.push_back((neighbor_idx, next_path));
                    }
                }
            }
        }

        None
    }

    /// 直接の子依存関係を取得
    pub fn direct_dependencies_of(&self, id: &PackageId) -> Vec<&PackageInfo> {
        if let Some(&idx) = self.node_indices.get(id) {
            self.graph
                .neighbors_directed(idx, Direction::Outgoing)
                .filter_map(|n_idx| self.graph.node_weight(n_idx))
                .collect()
        } else {
            Vec::new()
        }
    }
}

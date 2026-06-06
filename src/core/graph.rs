use std::collections::{HashMap, HashSet};
use anyhow::{Result, anyhow};

pub struct DepGraph {
    nodes: HashMap<String, Vec<String>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.nodes.entry(from.to_string()).or_default().push(to.to_string());
        self.nodes.entry(to.to_string()).or_default();
    }

    pub fn has_cycles(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        for node in self.nodes.keys() {
            if self.dfs_cycle(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    pub fn compute_topological_sort(&self) -> Result<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        for node in self.nodes.keys() {
            if !visited.contains(node) {
                if self.dfs_sort(node, &mut visited, &mut rec_stack, &mut order) {
                    return Err(anyhow!("Circular structural dependency loop detected"));
                }
            }
        }
        order.reverse();
        Ok(order)
    }

    fn dfs_cycle(&self, node: &str, visited: &mut HashSet<String>, rec_stack: &mut HashSet<String>) -> bool {
        if rec_stack.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        if let Some(neighbors) = self.nodes.get(node) {
            for neighbor in neighbors {
                if self.dfs_cycle(neighbor, visited, rec_stack) {
                    return true;
                }
            }
        }
        rec_stack.remove(node);
        false
    }

    fn dfs_sort(&self, node: &str, visited: &mut HashSet<String>, rec_stack: &mut HashSet<String>, order: &mut Vec<String>) -> bool {
        if rec_stack.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        if let Some(neighbors) = self.nodes.get(node) {
            for neighbor in neighbors {
                if self.dfs_sort(neighbor, visited, rec_stack, order) {
                    return true;
                }
            }
        }
        rec_stack.remove(node);
        order.push(node.to_string());
        false
    }
}
use super::query::{NodeRef, Pattern, Query, QueryMatch};

pub use glr_core::{Node, NodeId};

/// Iterator over query match results.
///
/// Walks the tree in DFS order, yielding one `QueryMatch` per (node, pattern)
/// pair that matches.
pub struct QueryMatches<'a> {
    tree: &'a glr_core::Tree,
    query: &'a Query,
    nodes: &'a [Node],
    // DFS traversal state using node arena indices
    stack: Vec<NodeId>,
    // Per-node state: which pattern we are currently trying
    next_pattern: usize,
    done: bool,
}

impl<'a> QueryMatches<'a> {
    fn new(tree: &'a glr_core::Tree, query: &'a Query) -> Self {
        let nodes = tree.nodes();
        let stack = if let Some(root) = tree.root_id() {
            vec![root]
        } else {
            Vec::new()
        };
        let done = stack.is_empty();
        QueryMatches {
            tree,
            query,
            nodes,
            stack,
            next_pattern: 0,
            done,
        }
    }

    /// Try to match `pattern` against `node`. Returns captured (capture_index, Node) pairs
    /// if the pattern matches, or `None` if it doesn't.
    fn try_match(
        &self,
        pattern: &Pattern,
        node: &Node,
        _node_id: NodeId,
    ) -> Option<Vec<(usize, NodeRef)>> {
        // 1. Check kind / wildcard
        match &pattern.kind {
            Some(expected_kind) => {
                let actual_name = self.tree.symbol_name(node.kind);
                if actual_name != expected_kind.as_str() {
                    return None;
                }
                // Named patterns only match named nodes
                if pattern.named && !node.flags.is_named() {
                    return None;
                }
            }
            None => {
                // Wildcard
                if pattern.named && !node.flags.is_named() {
                    return None;
                }
                // `*` matches anything
            }
        }

        let mut captures: Vec<(usize, NodeRef)> = Vec::new();

        // 2. Check field constraints
        for fc in &pattern.field_constraints {
            // Find the field id for this field name
            let field_id_opt = self.tree.field_names().iter().position(|n| n == &fc.name);
            match field_id_opt {
                Some(fid) => {
                    let fid = fid as u16;
                    let mut found = false;
                    for &child_id in &node.children {
                        if let Some(child) = self.nodes.get(child_id) {
                            if child.field_id == Some(fid) {
                                if let Some(mut child_caps) =
                                    self.try_match(&fc.pattern, child, child_id)
                                {
                                    captures.append(&mut child_caps);
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !found {
                        return None;
                    }
                }
                None => {
                    // Field name not found in tree's field names — cannot match
                    return None;
                }
            }
        }

        // 3. Check positional child patterns (in order)
        let mut child_idx = 0usize;
        for child_pattern in &pattern.child_patterns {
            let mut matched = false;
            while child_idx < node.children.len() {
                let child_id = node.children[child_idx];
                child_idx += 1;
                if let Some(child) = self.nodes.get(child_id) {
                    if let Some(mut child_caps) = self.try_match(child_pattern, child, child_id) {
                        captures.append(&mut child_caps);
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                return None;
            }
        }

        // 4. If this pattern has a capture, add it
        if let Some(cap_idx) = pattern.capture_index {
            captures.push((cap_idx, node.clone()));
        }

        Some(captures)
    }
}

impl<'a> Iterator for QueryMatches<'a> {
    type Item = QueryMatch;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            // Try remaining patterns for the current node
            while self.next_pattern < self.query.patterns.len() {
                let pattern = &self.query.patterns[self.next_pattern];
                self.next_pattern += 1;

                let node_id = *self.stack.last()?;
                let node = self.nodes.get(node_id)?;

                if let Some(captures) = self.try_match(pattern, node, node_id) {
                    let result = QueryMatch {
                        pattern_index: self.next_pattern - 1,
                        captures,
                    };
                    return Some(result);
                }
            }

            // Move to next node via DFS
            self.next_pattern = 0;
            let current = *self.stack.last()?;

            // Try first child
            if let Some(node) = self.nodes.get(current) {
                if let Some(&first_child) = node.children.first() {
                    self.stack.push(first_child);
                    continue;
                }
            }

            // No children — backtrack to find next sibling
            loop {
                let top = *self.stack.last()?;
                let parent_id = if self.stack.len() >= 2 {
                    Some(self.stack[self.stack.len() - 2])
                } else {
                    None
                };

                if let Some(pid) = parent_id {
                    if let Some(parent) = self.nodes.get(pid) {
                        // Find the index of `top` in parent's children
                        if let Some(pos) = parent.children.iter().position(|&c| c == top) {
                            if pos + 1 < parent.children.len() {
                                self.stack.pop();
                                self.stack.push(parent.children[pos + 1]);
                                break;
                            }
                        }
                    }
                }

                // No more siblings at this level — go up
                self.stack.pop();
                if self.stack.len() <= 1 {
                    // Back at root, no more siblings
                    self.done = true;
                    return None;
                }
                // Continue loop to check next sibling of new top
            }
        }
    }
}

pub trait Queryable {
    fn query<'a>(&'a self, query: &'a Query) -> QueryMatches<'a>;
}

impl Queryable for glr_core::Tree {
    fn query<'a>(&'a self, query: &'a Query) -> QueryMatches<'a> {
        QueryMatches::new(self, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_query;
    use glr_core::tree::NodeFlags;
    use glr_core::{SymbolId, Tree};

    fn symbol_id(n: u32) -> SymbolId {
        SymbolId(n)
    }

    fn make_node(kind: u32, children: Vec<NodeId>, field_id: Option<u16>, named: bool) -> Node {
        let mut flags = NodeFlags::new();
        flags.set_named(named);
        Node {
            kind: symbol_id(kind),
            start_byte: 0,
            end_byte: 0,
            start_position: (0, 0),
            end_position: (0, 0),
            children,
            flags,
            parser_state: None,
            field_id,
        }
    }

    #[test]
    fn test_simple_match() {
        let child = make_node(1, vec![], None, true);
        let root = make_node(0, vec![0], None, true);
        let nodes = vec![child, root];
        let tree = Tree::from_arena(nodes, Some(1))
            .with_symbol_names(vec!["program".into(), "identifier".into()]);

        let query = compile_query("(identifier) @id").unwrap();
        let results: Vec<QueryMatch> = tree.query(&query).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures.len(), 1);
        assert_eq!(query.captures[results[0].captures[0].0].name, "id");
    }

    #[test]
    fn test_no_match() {
        let root = make_node(0, vec![], None, true);
        let nodes = vec![root];
        let tree = Tree::from_arena(nodes, Some(0)).with_symbol_names(vec!["function".into()]);

        let query = compile_query("(identifier) @id").unwrap();
        let results: Vec<QueryMatch> = tree.query(&query).collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_field_match() {
        let id_node = make_node(1, vec![], Some(0), true);
        let func_node = make_node(0, vec![0], None, true);
        let nodes = vec![id_node, func_node];
        let tree = Tree::from_arena(nodes, Some(1))
            .with_symbol_names(vec!["function".into(), "identifier".into()])
            .with_field_names(vec!["name".into()]);

        let query = compile_query("(function name: (identifier) @n)").unwrap();
        let results: Vec<QueryMatch> = tree.query(&query).collect();
        assert_eq!(results.len(), 1, "should match the function node");
        assert_eq!(results[0].captures.len(), 1);
    }

    #[test]
    fn test_wildcard() {
        let child = make_node(0, vec![], None, true);
        let root = make_node(0, vec![0], None, true);
        let nodes = vec![child, root];
        let tree = Tree::from_arena(nodes, Some(1)).with_symbol_names(vec!["node".into()]);

        let query = compile_query("(_) @all").unwrap();
        let results: Vec<QueryMatch> = tree.query(&query).collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_star_wildcard() {
        let mut flags = NodeFlags::new();
        flags.set_named(false);
        let anon = Node {
            kind: symbol_id(0),
            start_byte: 0,
            end_byte: 0,
            start_position: (0, 0),
            end_position: (0, 0),
            children: vec![],
            flags,
            parser_state: None,
            field_id: None,
        };
        let nodes = vec![anon];
        let tree = Tree::from_arena(nodes, Some(0)).with_symbol_names(vec!["anon".into()]);

        // `(*)` matches any node (including anonymous)
        let query = compile_query("(*) @any").unwrap();
        let results: Vec<QueryMatch> = tree.query(&query).collect();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_nested_pattern() {
        let str_node = make_node(2, vec![], None, true);
        let id_node = make_node(1, vec![], None, true);
        let pair_node = make_node(0, vec![1, 0], None, true);
        let nodes = vec![str_node, id_node, pair_node];
        let tree = Tree::from_arena(nodes, Some(2)).with_symbol_names(vec![
            "pair".into(),
            "identifier".into(),
            "string".into(),
        ]);

        let query = compile_query("(pair (identifier) (string))").unwrap();
        let results: Vec<QueryMatch> = tree.query(&query).collect();
        assert_eq!(results.len(), 1);
    }
}

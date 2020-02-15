use std::collections::HashMap;

use petgraph::graphmap::DiGraphMap;

use crate::types::{ NodeType, EdgeType };

/// The main PageGraph data structure.
#[derive(Debug)]
pub struct PageGraph {
    pub edges: HashMap<EdgeId, Edge>,
    pub nodes: HashMap<NodeId, Node>,
    pub graph: DiGraphMap<NodeId, EdgeId>,
}

/// An identifier used to reference a node.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
pub struct NodeId(usize);

impl From<usize> for NodeId {
    fn from(v: usize) -> Self {
        NodeId(v)
    }
}

/// A node, representing a side effect of a page load.
#[derive(Debug)]
pub struct Node {
    pub node_timestamp: isize,
    pub node_type: NodeType,
}

/// An identifier used to reference an edge.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
pub struct EdgeId(usize);

impl From<usize> for EdgeId {
    fn from(v: usize) -> Self {
        EdgeId(v)
    }
}

/// An edge, representing an action taken during page load.
#[derive(Debug)]
pub struct Edge {
    pub edge_timestamp: Option<isize>,
    pub edge_type: EdgeType,
}

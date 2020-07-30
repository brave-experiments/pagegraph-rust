use crate::graph::{Edge, EdgeId, Node, NodeId, PageGraph};
use crate::types::{EdgeType, NodeType};

use petgraph::Direction;

pub type NodeRef<'a> = (NodeId, &'a Node);

impl PageGraph {
    pub fn filter_edges<F: Fn(&EdgeType) -> bool>(&self, f: F) -> Vec<(&EdgeId, &Edge)> {
        self.edges
            .iter()
            .filter(|(_id, edge)| f(&edge.edge_type))
            .collect()
    }

    pub fn filter_nodes<F: Fn(&NodeType) -> bool>(&self, f: F) -> Vec<NodeRef> {
        self.nodes
            .iter()
            .filter(|(_id, node)| f(&node.node_type))
            .map(|(i, n)| (*i, n))
            .collect()
    }

    pub fn nodes_of_html_tag_name(&self, tag_name: &str) -> Vec<NodeRef> {
        self.nodes
            .iter()
            .filter(|(_id, node)| match &node.node_type {
                NodeType::HtmlElement { tag_name: tn, .. } => tn == tag_name,
                _ => false,
            })
            .map(|(i, n)| (*i, n))
            .collect()
    }

    pub fn cause_of(&self, node_ref: &NodeRef) -> Option<NodeRef> {
        match node_ref.1.node_type {
            NodeType::TextNode { .. } | NodeType::HtmlElement { .. } => None,
            _ => None,
        }
    }

    /// Returns a sorted Vec including 1 edge representing every time the given HtmlElement node was
    /// modified in the page.
    pub fn all_html_element_modifications(&self, node_id: NodeId) -> Vec<(&EdgeId, &Edge)> {
        let element = self.nodes.get(&node_id).unwrap();

        if let NodeType::HtmlElement {
            node_id: _html_node_id,
            ..
        } = element.node_type
        {
            let mut modifications: Vec<_> = self
                .graph
                .neighbors_directed(node_id, Direction::Incoming)
                .map(|node| {
                    let edge_id = self.graph.edge_weight(node, node_id).unwrap();
                    (edge_id, self.edges.get(edge_id).unwrap())
                })
                .filter(|(_id, edge)| match edge.edge_type {
                    EdgeType::Structure { .. } => false,
                    _ => true,
                })
                .collect();

            modifications.sort_by_key(|(_, edge)| {
                edge.edge_timestamp
                    .expect("HTML element modification had no timestamp")
            });

            modifications
        } else {
            panic!("Supply a node with HtmlElement node type");
        }
    }

    /// Get a collection of all Resource nodes whose requests were initiated by a given Script node or HtmlElement node with tag_name "script".
    ///
    /// For script nodes, associated resources are directly attached by a Request Start edge.
    /// For script HTML element nodes, associated resources are either directly attached by a
    /// Request Start edge (`src="..."`), or additionally by a Request Start edge on an attached
    /// Script node.
    pub fn resources_from_script(&self, node_id: NodeId) -> Vec<(NodeId, &Node)> {
        let element = self.nodes.get(&node_id).unwrap();

        let mut resulting_resources: Vec<NodeId> = self
            .graph
            .neighbors_directed(node_id, Direction::Outgoing)
            .filter(
                |neighbor_id| match self.nodes.get(&neighbor_id).unwrap().node_type {
                    NodeType::Resource { .. } => true,
                    _ => false,
                },
            )
            .collect();

        match element.node_type {
            NodeType::Script { .. } => (),
            NodeType::HtmlElement { ref tag_name, .. } if tag_name == "script" => {
                let mut resources_from_executed_script: Vec<NodeId> = self
                    .graph.neighbors_directed(node_id, Direction::Outgoing)
                    .filter(|neighbor_id| match self.nodes.get(&neighbor_id).unwrap().node_type { NodeType::Script { .. } => true, _ => false })
                    .map(|attached_script_id| self.graph.neighbors_directed(attached_script_id, Direction::Outgoing))
                    .flatten()
                    .filter(|attached_script_neighbor_id| match self.nodes.get(attached_script_neighbor_id).unwrap().node_type { NodeType::Resource { .. } => true, _ => false })
                    .collect();
                resulting_resources.append(&mut resources_from_executed_script);
            },
            _ => panic!("Supply a node with Script node type, or an HtmlElement node with tag_name \"script\""),
        }

        resulting_resources
            .into_iter()
            .map(|node_id| (node_id, self.nodes.get(&node_id).unwrap()))
            .collect()
    }

    /// Gets the URL of the page the graph was recorded from
    pub fn root_url(&self) -> String {
        let root_urls = self
            .nodes
            .iter()
            .filter(|(node_id, node)| {
                if let NodeType::DomRoot { .. } = node.node_type {
                    self.graph
                        .edges_directed(*node_id.to_owned(), Direction::Incoming)
                        .count()
                        == 0
                } else {
                    false
                }
            })
            .map(|(_, node)| match &node.node_type {
                NodeType::DomRoot { url: Some(url), .. } => url,
                _ => panic!("Could not find DOM root URL"),
            })
            .collect::<Vec<_>>();
        assert_eq!(root_urls.len(), 1);
        return root_urls[0].to_string();
    }

    /// Get a collection of all Resource nodes whose requests match a given adblock filter pattern.
    pub fn resources_matching_filter(&self, pattern: &str) -> Vec<NodeRef> {
        let source_url = self.root_url();

        let source_url = url::Url::parse(&source_url).expect("Could not parse source URL");
        let source_hostname = source_url
            .host_str()
            .expect(&format!("Source URL has no host, {:?}", source_url));
        let source_domain = get_domain(source_hostname);

        if let Ok(rule) = adblock::filters::network::NetworkFilter::parse(pattern, false) {
            self.nodes
                .iter()
                .filter(|(id, node)| match &node.node_type {
                    NodeType::Resource { url } => {
                        use adblock::filters::network::NetworkMatchable as _;

                        let request_url = match url::Url::parse(url) {
                            Ok(request_url) => request_url,
                            Err(_) => return false,
                        };
                        let request_url_scheme = request_url.scheme();
                        let request_url_hostname = request_url.host_str().expect("Request URL has no host");
                        let request_url_domain = get_domain(request_url_hostname);

                        let request_start_edges = self.graph
                            .edges_directed(*id.to_owned(), Direction::Incoming)
                            .filter(|(_, _, edge_id)| match &self.edges.get(edge_id).unwrap().edge_type {
                                EdgeType::RequestStart { .. } => true,
                                _ => false,
                            });
                        let mut unique_request_types = request_start_edges.map(|(_, _, edge_id)|
                                if let Some(Edge { edge_type: EdgeType::RequestStart { request_type, .. }, .. }) = self.edges.get(edge_id) {
                                    request_type.to_owned()
                                } else {
                                    unreachable!()
                                }
                            ).collect::<std::collections::HashSet<_>>()
                            .into_iter().collect::<Vec<_>>();
                        if unique_request_types.len() == 0 {
                            panic!("Resource did not have a corresponding RequestStart edge");
                        } else if unique_request_types.len() > 1 {
                            panic!("Resource was requested with multiple different request types: {:?}", unique_request_types);
                        }
                        let request_type = unique_request_types.remove(0);

                        let request = adblock::request::Request::new(
                            &request_type,
                            url,
                            request_url_scheme,
                            request_url_hostname,
                            &request_url_domain,
                            source_hostname,
                            &source_domain,
                        );
                        rule.matches(&request)
                    }
                    _ => false
                })
                .map(|(id, node)| (*id, node))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn direct_downstream_effects_of(&self, node_id: NodeId) -> Vec<NodeRef> {
        match &self.nodes.get(&node_id).unwrap().node_type {
            NodeType::Extensions {} => unimplemented!(),
            NodeType::RemoteFrame { .. } => unimplemented!(),
            NodeType::Resource { .. } => {
                // script resources cause the execution of the corresponding script, which is connected
                // through the corresponding HTML script element.
                let attached_script_elements = self
                    .graph
                    .edges_directed(node_id, Direction::Outgoing)
                    .filter_map(|(_n0, n1, edge_id)| {
                        match &self.edges.get(&edge_id).unwrap().edge_type {
                            EdgeType::RequestComplete { .. } => Some(n1),
                            _ => None,
                        }
                    })
                    .filter(
                        |target_node| match &self.nodes.get(target_node).unwrap().node_type {
                            NodeType::HtmlElement { tag_name, .. } if tag_name == "script" => true,
                            _ => false,
                        },
                    )
                    .collect::<Vec<_>>();

                attached_script_elements
                    .into_iter()
                    .map(|html_node_id| {
                        self.graph
                            .neighbors_directed(html_node_id, Direction::Outgoing)
                            .filter(|html_neighbor| {
                                match &self.nodes.get(html_neighbor).unwrap().node_type {
                                    NodeType::Script { .. } => true,
                                    _ => false,
                                }
                            })
                    })
                    .flatten()
                    .map(|script_node_id| {
                        (script_node_id, self.nodes.get(&script_node_id).unwrap())
                    })
                    .collect::<Vec<_>>()
            }
            NodeType::AdFilter { .. } => unimplemented!(),
            NodeType::TrackerFilter => unimplemented!(), // TODO
            NodeType::FingerprintingFilter => unimplemented!(), // TODO
            NodeType::WebApi { .. } => unimplemented!(),
            NodeType::JsBuiltin { .. } => unimplemented!(),
            NodeType::HtmlElement { tag_name, .. } if tag_name == "script" => {
                // script elements with a src attribute cause a resource request
                let resource_requests = self
                    .graph
                    .neighbors_directed(node_id, Direction::Outgoing)
                    .filter(
                        |node_id| match &self.nodes.get(&node_id).unwrap().node_type {
                            NodeType::Resource { .. } => true,
                            _ => false,
                        },
                    )
                    .map(|node_id| (node_id, self.nodes.get(&node_id).unwrap()))
                    .collect::<Vec<_>>();

                // inline script elements cause a script execution
                if resource_requests.is_empty() {
                    self.graph
                        .neighbors_directed(node_id, Direction::Outgoing)
                        .filter(
                            |node_id| match &self.nodes.get(&node_id).unwrap().node_type {
                                NodeType::Script { .. } => true,
                                _ => false,
                            },
                        )
                        .map(|node_id| (node_id, self.nodes.get(&node_id).unwrap()))
                        .collect::<Vec<_>>()
                } else {
                    resource_requests
                }
            }
            NodeType::HtmlElement { tag_name: _, .. } => unimplemented!(),
            NodeType::TextNode { .. } => unimplemented!(),
            NodeType::DomRoot { .. } => unimplemented!(),
            NodeType::FrameOwner { .. } => unimplemented!(),
            NodeType::Storage {} => unimplemented!(),
            NodeType::LocalStorage {} => unimplemented!(),
            NodeType::SessionStorage {} => unimplemented!(),
            NodeType::CookieJar {} => unimplemented!(),
            NodeType::Script { .. } => {
                // scripts can fetch resources
                let fetched_resources = self
                    .graph
                    .edges_directed(node_id, Direction::Incoming)
                    .filter_map(|(n0, _n1, edge_id)| {
                        match &self.edges.get(&edge_id).unwrap().edge_type {
                            EdgeType::RequestComplete { .. } => Some(n0),
                            _ => None,
                        }
                    })
                    .map(|node_id| (node_id, self.nodes.get(&node_id).unwrap()));

                // scripts can execute other scripts
                let executed_scripts = self
                    .graph
                    .neighbors_directed(node_id, Direction::Outgoing)
                    .filter(
                        |node_id| match &self.nodes.get(&node_id).unwrap().node_type {
                            NodeType::Script { .. } => true,
                            _ => false,
                        },
                    )
                    .map(|node_id| (node_id, self.nodes.get(&node_id).unwrap()));

                fetched_resources
                    .chain(executed_scripts)
                    .collect::<Vec<_>>()
                // TODO scripts can create/modify/insert DOM elements, execute web APIs and JS
                // builtins, build 3rd party frames, access storage, access cookies...
            }
            NodeType::Parser {} => unimplemented!(),
            NodeType::BraveShields {} => unimplemented!(),
            NodeType::AdsShield {} => unimplemented!(),
            NodeType::TrackersShield {} => unimplemented!(),
            NodeType::JavascriptShield {} => unimplemented!(),
            NodeType::FingerprintingShield {} => unimplemented!(),
        }
    }

    pub fn all_downstream_effects_of(&self, init_node_id: NodeId) -> Vec<NodeRef> {
        let mut nodes_to_check: Vec<NodeId> = vec![init_node_id];
        let mut already_checked: Vec<NodeId> = vec![];

        while let Some(node_id) = nodes_to_check.pop() {
            let direct_effects = self.direct_downstream_effects_of(node_id);
            already_checked.push(node_id);

            direct_effects.into_iter().for_each(|(inner_node_id, _)| {
                if !already_checked.contains(&inner_node_id) {
                    nodes_to_check.push(node_id);
                }
            });
        }

        already_checked
            .into_iter()
            .map(|node_id| (node_id, self.nodes.get(&node_id).unwrap()))
            .collect()
    }
}

fn get_domain(host: &str) -> String {
    let source_hostname = host;
    let source_domain = source_hostname
        .parse::<addr::DomainName>()
        .expect("Source URL domain could not be parsed");
    let source_domain =
        &source_hostname[source_hostname.len() - source_domain.root().to_str().len()..];
    source_domain.to_string()
}

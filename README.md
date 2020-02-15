# pagegraph

This crate provides utilities for analyzing PageGraph outputs.

## Example

The following example reads from a PageGraph file and produces all deleted
`div` elements from the corresponding webpage.

```rust
use pagegraph::from_xml::read_from_file;
use pagegraph::types::{ NodeType, EdgeType };

fn main() {
    let graph = read_from_file("/path/to/any/pagegraph.graphml");

    let deleted_divs = graph.filter_nodes(|node| {
        match node {
            NodeType::HtmlElement { is_deleted: true, tag_name, .. } if tag_name == "div" => true,
            _ => false,
        }
    });
}

```

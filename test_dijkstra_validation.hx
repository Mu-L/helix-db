// Test file to verify ShortestPathDijkstras validation

// Valid: Simple property access
QUERY validWeightProperty(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathDijkstras<Road>(_::{distance})::To(to)
RETURN path

// Invalid: Complex expression (should error)
QUERY invalidComplexExpression(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathDijkstras<Road>(_::{distance * 2})::To(to)
RETURN path

// Invalid: Multiple properties (should error) 
QUERY invalidMultipleProps(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathDijkstras<Road>(_::{distance, time})::To(to)
RETURN path

// Invalid: Nested traversal (should error)
QUERY invalidNestedTraversal(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathDijkstras<Road>(_::Out::First)::To(to)
RETURN path

// Valid: BFS doesn't require weight
QUERY validBFS(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathBFS<Road>::To(to)
RETURN path
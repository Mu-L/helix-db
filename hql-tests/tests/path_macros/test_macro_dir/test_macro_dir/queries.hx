QUERY testMacro(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathDijkstras<Road>(_::{weight})::To(to)
RETURN path
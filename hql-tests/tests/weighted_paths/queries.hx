// Queries for testing weighted shortest path algorithms
// These demonstrate scenarios where Dijkstra finds better paths than BFS

// ─── City/Highway Network Tests ─────────────────────────────────

// Find shortest path by distance
QUERY shortestDistance(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathDijkstras<Highway>(_::{weight})::To(to)
RETURN path

// Find path with fewest cities (BFS)
QUERY fewestCities(from: ID, to: ID) =>
    path <- N<City>(from)::ShortestPathBFS<Highway>::To(to)
RETURN path

// Compare both algorithms
QUERY compareRoutes(from: ID, to: ID) =>
    shortest_distance <- N<City>(from)::ShortestPathDijkstras<Highway>(_::{weight})::To(to)
    fewest_stops <- N<City>(from)::ShortestPathBFS<Highway>::To(to)
RETURN {
    by_distance: shortest_distance,
    by_stops: fewest_stops
}

// ─── Network Routing Tests ──────────────────────────────────────

// Find lowest latency path
QUERY lowestLatency(source: ID, dest: ID) =>
    path <- N<NetworkNode>(source)::ShortestPathDijkstras<NetworkLink>(_::{weight})::To(dest)
RETURN path

// Find path with fewest hops (traditional routing)
QUERY fewestHops(source: ID, dest: ID) =>
    path <- N<NetworkNode>(source)::ShortestPathBFS<NetworkLink>::To(dest)
RETURN path

// Multi-criteria network path
QUERY networkPathAnalysis(source: ID, dest: ID) =>
    latency_path <- N<NetworkNode>(source)::ShortestPathDijkstras<NetworkLink>(_::{weight})::To(dest)
    hop_path <- N<NetworkNode>(source)::ShortestPathBFS<NetworkLink>::To(dest)
    nodes <- N<NetworkNode>()
    links <- E<NetworkLink>()
RETURN {
    optimal_latency: latency_path,
    minimum_hops: hop_path,
    all_nodes: nodes,
    all_links: links
}

// ─── Supply Chain Optimization ──────────────────────────────────

// Find fastest delivery route
QUERY fastestDelivery(origin: ID, destination: ID) =>
    path <- N<Warehouse>(origin)::ShortestPathDijkstras<SupplyRoute>(_::{weight})::To(destination)
RETURN path

// Find most cost-effective route
QUERY cheapestRoute(origin: ID, destination: ID) =>
    path <- N<Warehouse>(origin)::ShortestPathDijkstras<SupplyRoute>(_::{weight})::To(destination)
RETURN path

// Find route with fewest transfers
QUERY fewestTransfers(origin: ID, destination: ID) =>
    path <- N<Warehouse>(origin)::ShortestPathBFS<SupplyRoute>::To(destination)
RETURN path

// Environmental impact analysis
QUERY greenestRoute(origin: ID, destination: ID) =>
    dijkstra_path <- N<Warehouse>(origin)::ShortestPathDijkstras<SupplyRoute>(_::{weight})::To(destination)
    bfs_path <- N<Warehouse>(origin)::ShortestPathBFS<SupplyRoute>::To(destination)
RETURN {
    lowest_emissions: dijkstra_path,
    fewest_segments: bfs_path
}

// ─── Power Grid Analysis ────────────────────────────────────────

// Find path with lowest resistance
QUERY lowestResistance(source: ID, target: ID) =>
    path <- N<PowerStation>(source)::ShortestPathDijkstras<PowerLine>(_::{weight})::To(target)
RETURN path

// Find path with fewest substations
QUERY directPower(source: ID, target: ID) =>
    path <- N<PowerStation>(source)::ShortestPathBFS<PowerLine>::To(target)
RETURN path

// Power grid redundancy analysis
QUERY gridRedundancy(station1: ID, station2: ID) =>
    primary_path <- N<PowerStation>(station1)::ShortestPathDijkstras<PowerLine>(_::{weight})::To(station2)
    backup_path <- N<PowerStation>(station1)::ShortestPathBFS<PowerLine>::To(station2)
RETURN {
    primary: primary_path,
    backup: backup_path
}

// ─── Advanced Path Queries ──────────────────────────────────────

// Bidirectional Dijkstra test
QUERY bidirectionalDijkstra(nodeA: ID, nodeB: ID) =>
    forward <- N<City>(nodeA)::ShortestPathDijkstras<Highway>(_::{weight})::To(nodeB)
    backward <- N<City>(nodeB)::ShortestPathDijkstras<Highway>(_::{weight})::From(nodeA)
RETURN {
    forward_path: forward,
    backward_path: backward
}

// Multiple shortest paths from single source
QUERY multiDestination(source: ID, dest1: ID, dest2: ID, dest3: ID) =>
    path1 <- N<City>(source)::ShortestPathDijkstras<Highway>(_::{weight})::To(dest1)
    path2 <- N<City>(source)::ShortestPathDijkstras<Highway>(_::{weight})::To(dest2)
    path3 <- N<City>(source)::ShortestPathDijkstras<Highway>(_::{weight})::To(dest3)
RETURN {
    to_dest1: path1,
    to_dest2: path2,
    to_dest3: path3
}

// Path existence check
QUERY pathExists(from: ID, to: ID) =>
    dijkstra_exists <- N<City>(from)::ShortestPathDijkstras<Highway>(_::{weight})::To(to)
    bfs_exists <- N<City>(from)::ShortestPathBFS<Highway>::To(to)
RETURN {
    dijkstra_found: dijkstra_exists,
    bfs_found: bfs_exists
}

// Constrained shortest path (combining with WHERE)
QUERY constrainedPath(from: ID, to: ID, max_toll: F64) =>
    cities <- N<City>(from)::ShortestPathDijkstras<Highway>(_::{weight})::To(to)
    highways <- E<Highway>() WHERE toll_cost_usd <= max_toll
RETURN cities, highways
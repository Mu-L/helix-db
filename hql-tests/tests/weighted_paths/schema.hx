// Schema for testing weighted shortest path algorithms
// Focus on scenarios where weight matters significantly

N::City {
    INDEX name: String,
    country: String,
    population: I64,
    timezone: String,
}

// Road network with multiple weight factors
E::Highway {
    From: City,
    To: City,
    Properties: {
        distance_km: F64,
        travel_time_hours: F64,
        toll_cost_usd: F64,
        traffic_level: I32,      // 1-10, affects actual travel time
        road_condition: I32,     // 1-10, affects safety/comfort
    }
}

// Network graph for testing algorithm performance
N::NetworkNode {
    INDEX ip_address: String,
    hostname: String,
    node_type: String,           // router, switch, server, etc.
}

E::NetworkLink {
    From: NetworkNode,
    To: NetworkNode,
    Properties: {
        bandwidth_mbps: F64,
        latency_ms: F64,
        packet_loss: F64,        // Percentage 0-100
        cost_per_gb: F64,
        reliability: F64,        // 0-1 score
    }
}

// Supply chain network
N::Warehouse {
    INDEX code: String,
    location: String,
    capacity: I32,
    type: String,                // distribution, storage, manufacturing
}

E::SupplyRoute {
    From: Warehouse,
    To: Warehouse,
    Properties: {
        distance_km: F64,
        shipping_time_days: F64,
        cost_per_kg: F64,
        max_capacity_kg: F64,
        carbon_emissions_kg: F64,
        reliability_score: F64,  // 0-1, probability of on-time delivery
    }
}

// Utility/Power grid network
N::PowerStation {
    INDEX station_id: String,
    name: String,
    type: String,                // coal, nuclear, solar, wind, hydro
    max_output_mw: F64,
}

E::PowerLine {
    From: PowerStation,
    To: PowerStation,
    Properties: {
        resistance_ohms: F64,    // Electrical resistance
        max_capacity_mw: F64,    // Maximum power transfer
        line_loss_percent: F64,  // Power loss during transmission
        maintenance_cost: F64,   // Annual maintenance cost
        age_years: I32,          // Age affects reliability
    }
}
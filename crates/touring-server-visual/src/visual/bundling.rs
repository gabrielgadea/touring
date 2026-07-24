//! Force-Directed Edge Bundling (FDEB) via Holten/van Wijk algorithm.
//!
//! Implements the force-directed edge bundling algorithm described in:
//! "Force-Directed Edge Bundling" by Holten & van Wijk (2009).
//! Provides control-point-based edge bundling with compatibility filtering
//! for graph visualization.
//!
//! # Algorithm Parameters
//!
//! - `control_points`: 20 points per edge (default)
//! - `iterations`: 6 iterations typical
//! - `compatibility thresholds`: angular (~0.3), scale (~0.5), position (~0.5), visibility (~0.5)

use serde::{Deserialize, Serialize};

/// 2D point for geometric computations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl Point {
    /// Create a new point.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Distance to another point.
    pub fn distance_to(self, other: Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Squared distance to another point (avoids sqrt for comparisons).
    pub fn distance_squared_to(self, other: Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    /// Add two points component-wise.
    #[allow(clippy::should_implement_trait)] // geometric component-wise add, not std::ops::Add
    pub fn add(self, other: Point) -> Point {
        Point::new(self.x + other.x, self.y + other.y)
    }

    /// Subtract two points component-wise.
    pub fn subtract(self, other: Point) -> Point {
        Point::new(self.x - other.x, self.y - other.y)
    }

    /// Multiply point by scalar.
    pub fn scale(self, factor: f64) -> Point {
        Point::new(self.x * factor, self.y * factor)
    }

    /// Compute the midpoint between two points.
    pub fn midpoint(self, other: Point) -> Point {
        Point::new((self.x + other.x) / 2.0, (self.y + other.y) / 2.0)
    }

    /// Linear interpolation between two points.
    pub fn lerp(self, other: Point, t: f64) -> Point {
        Point::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    /// Magnitude (length) of the vector from origin to this point.
    pub fn magnitude(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Normalize to unit vector. Returns zero vector if magnitude is near zero.
    pub fn normalize(self) -> Point {
        let mag = self.magnitude();
        if mag < 1e-10 {
            Point::new(0.0, 0.0)
        } else {
            Point::new(self.x / mag, self.y / mag)
        }
    }

    /// Dot product of two points (treating them as vectors from origin).
    pub fn dot(self, other: Point) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Cross product (z-component of 3D cross product, treating points as 2D vectors).
    pub fn cross(self, other: Point) -> f64 {
        self.x * other.y - self.y * other.x
    }
}

/// Edge in the graph for bundling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    /// Source node index.
    pub from: usize,
    /// Target node index.
    pub to: usize,
}

/// Node in the graph for bundling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Node position.
    pub position: Point,
}

/// Compatibility thresholds for edge grouping.
///
/// Edges are considered compatible for bundling if their compatibility
/// score exceeds all thresholds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompatibilityThresholds {
    /// Angular compatibility threshold (default ~0.3).
    pub angular: f64,
    /// Scale compatibility threshold (default ~0.5).
    pub scale: f64,
    /// Position compatibility threshold (default ~0.5).
    pub position: f64,
    /// Visibility compatibility threshold (default ~0.5).
    pub visibility: f64,
}

impl Default for CompatibilityThresholds {
    fn default() -> Self {
        Self {
            angular: 0.3,
            scale: 0.5,
            position: 0.5,
            visibility: 0.5,
        }
    }
}

/// Configuration for edge bundling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BundlingConfig {
    /// Number of control points per edge (default 20).
    pub control_points: usize,
    /// Number of iterations (default 6).
    pub iterations: usize,
    /// Compatibility thresholds.
    pub compatibility_thresholds: CompatibilityThresholds,
}

impl Default for BundlingConfig {
    fn default() -> Self {
        Self {
            control_points: 20,
            iterations: 6,
            compatibility_thresholds: CompatibilityThresholds::default(),
        }
    }
}

/// Compatibility score between two edges.
#[derive(Debug, Clone, Copy)]
pub struct CompatibilityScore {
    /// Angular compatibility [0, 1].
    pub angular: f64,
    /// Scale compatibility [0, 1].
    pub scale: f64,
    /// Position compatibility [0, 1].
    pub position: f64,
    /// Visibility compatibility [0, 1].
    pub visibility: f64,
    /// Combined compatibility [0, 1].
    pub combined: f64,
}

impl CompatibilityScore {
    /// Check if edges are compatible given thresholds.
    pub fn is_compatible(&self, thresholds: &CompatibilityThresholds) -> bool {
        self.angular >= thresholds.angular
            && self.scale >= thresholds.scale
            && self.position >= thresholds.position
            && self.visibility >= thresholds.visibility
    }
}

/// Internal edge data for the bundling algorithm.
#[derive(Debug, Clone)]
struct EdgeData {
    /// Control points along the edge.
    control_points: Vec<Point>,
    /// Original source point.
    source: Point,
    /// Original target point.
    target: Point,
    /// Direction vector (normalized).
    direction: Point,
    /// Length of the edge.
    length: f64,
}

impl EdgeData {
    /// Create new edge data from two nodes.
    fn new(source: Point, target: Point, control_points: usize) -> Self {
        let direction = target.subtract(source).normalize();
        let length = source.distance_to(target);
        let control_points = Self::initialize_control_points(source, target, control_points);
        Self {
            control_points,
            source,
            target,
            direction,
            length,
        }
    }

    /// Initialize control points evenly spaced along the edge.
    fn initialize_control_points(source: Point, target: Point, n: usize) -> Vec<Point> {
        if n < 2 {
            return vec![source, target];
        }
        (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                source.lerp(target, t)
            })
            .collect()
    }

    /// Compute the spring force on a control point.
    fn spring_force(&self, idx: usize, k: f64) -> Point {
        if idx == 0 || idx == self.control_points.len() - 1 {
            return Point::new(0.0, 0.0);
        }
        let prev = self.control_points[idx - 1];
        let curr = self.control_points[idx];
        let next = self.control_points[idx + 1];

        // Attraction to previous point
        let f_prev = curr.subtract(prev).scale(k);
        // Attraction to next point
        let f_next = curr.subtract(next).scale(k);

        f_prev.add(f_next)
    }

    /// Compute the electrostatic repulsion force on a control point.
    fn repulsion_force(&self, other: &EdgeData, idx: usize, repulsion: f64) -> Point {
        let mut total_force = Point::new(0.0, 0.0);
        for other_point in other.control_points.iter() {
            let diff = self.control_points[idx].subtract(*other_point);
            let dist_sq = diff.x * diff.x + diff.y * diff.y;
            let dist = dist_sq.sqrt().max(1e-6);
            let force_mag = repulsion / dist_sq;
            total_force = total_force.add(diff.scale(force_mag / dist));
        }
        total_force
    }
}

/// Force-Directed Edge Bundling result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlingResult {
    /// Bundled edges, each represented as a vector of control points.
    pub bundled_edges: Vec<Vec<Point>>,
    /// Number of iterations performed.
    pub iterations_completed: usize,
    /// Initial edge count.
    pub edge_count: usize,
}

/// Compute compatibility score between two edges.
fn compute_compatibility(
    edge1: &EdgeData,
    edge2: &EdgeData,
    _thresholds: &CompatibilityThresholds,
) -> CompatibilityScore {
    // Angular compatibility: based on angle between edge directions
    let angle1 = edge1.direction;
    let angle2 = edge2.direction;
    let dot = angle1.dot(angle2).abs();
    let angular = dot;

    // Scale compatibility: based on ratio of edge lengths
    let len1 = edge1.length;
    let len2 = edge2.length;
    let scale_score = if len1.max(len2) > 1e-10 {
        let min_len = len1.min(len2);
        let max_len = len1.max(len2);
        min_len / max_len
    } else {
        0.0
    };
    let scale = scale_score;

    // Position compatibility: based on distance between edge midpoints
    let midpoint1 = edge1.source.midpoint(edge1.target);
    let midpoint2 = edge2.source.midpoint(edge2.target);
    let mid_dist = midpoint1.distance_to(midpoint2);
    let avg_len = (len1 + len2) / 2.0;
    let position = if avg_len > 1e-10 {
        1.0 - (mid_dist / (2.0 * avg_len)).min(1.0)
    } else {
        0.0
    };

    // Visibility compatibility: based on overlap when projected
    let visibility = compute_visibility_compatibility(edge1, edge2);

    // Combined score (geometric mean)
    let combined = (angular * scale * position * visibility).cbrt();

    CompatibilityScore {
        angular,
        scale,
        position,
        visibility,
        combined,
    }
}

/// Compute visibility compatibility between two edges.
fn compute_visibility_compatibility(edge1: &EdgeData, edge2: &EdgeData) -> f64 {
    // Project edge1's control points onto edge2's direction and check overlap
    let dir2 = edge2.target.subtract(edge2.source).normalize();

    let proj1_start = edge1.source.subtract(edge2.source).dot(dir2);
    let proj1_end = edge1.target.subtract(edge2.source).dot(dir2);
    let proj2_start = 0.0_f64;
    let proj2_end = edge2.source.distance_to(edge2.target);

    let min1 = proj1_start.min(proj1_end);
    let max1 = proj1_start.max(proj1_end);
    let min2 = proj2_start.min(proj2_end);
    let max2 = proj2_start.max(proj2_end);

    let overlap = (max1.min(max2) - min1.max(min2)).max(0.0);
    let union = (max1.max(max2) - min1.min(min2)).max(1e-10);

    overlap / union
}

/// Force-directed edge bundling algorithm.
///
/// Implements the Holten/van Wijk force-directed edge bundling algorithm.
/// Each edge is represented by a set of control points that are iteratively
/// repositioned based on attractive forces (springs along edges) and
/// repulsive forces (electrostatic between nearby edges).
///
/// # Parameters
///
/// - `edges`: Slice of edges with source/target node indices
/// - `nodes`: Slice of nodes with positions
/// - `config`: Bundling configuration
///
/// # Returns
///
/// Vector of bundled edges, each as a vector of control points.
pub fn force_directed_edge_bundling(
    edges: &[Edge],
    nodes: &[Node],
    config: BundlingConfig,
) -> BundlingResult {
    let edge_count = edges.len();
    if edge_count == 0 || nodes.is_empty() {
        return BundlingResult {
            bundled_edges: vec![],
            iterations_completed: 0,
            edge_count: 0,
        };
    }

    let n_control = config.control_points.max(2);

    // Initialize edge data
    let mut edge_data: Vec<EdgeData> = edges
        .iter()
        .map(|e| {
            let source = nodes[e.from].position;
            let target = nodes[e.to].position;
            EdgeData::new(source, target, n_control)
        })
        .collect();

    // Build compatibility graph
    let compatibility: Vec<Vec<bool>> = (0..edge_count)
        .map(|i| {
            (0..edge_count)
                .map(|j| {
                    if i == j {
                        false
                    } else {
                        let score = compute_compatibility(
                            &edge_data[i],
                            &edge_data[j],
                            &config.compatibility_thresholds,
                        );
                        score.is_compatible(&config.compatibility_thresholds)
                    }
                })
                .collect()
        })
        .collect();

    // Bundling iterations
    let k = 0.1; // Spring constant
    let repulsion = 0.1; // Repulsion constant

    for _iter in 0..config.iterations {
        // For each edge
        for i in 0..edge_count {
            let mut total_force = Point::new(0.0, 0.0);

            // Spring forces along the edge
            for j in 1..edge_data[i].control_points.len() - 1 {
                let spring = edge_data[i].spring_force(j, k);
                total_force = total_force.add(spring);
            }

            // Repulsion from compatible edges
            for j in 0..edge_count {
                if compatibility[i][j] {
                    for (cp_idx, _cp) in edge_data[i].control_points.iter().enumerate() {
                        if cp_idx > 0 && cp_idx < edge_data[i].control_points.len() - 1 {
                            let rep =
                                edge_data[j].repulsion_force(&edge_data[i], cp_idx, repulsion);
                            total_force = total_force.add(rep);
                        }
                    }
                }
            }

            // Apply forces to control points (except endpoints)
            let move_factor = 0.1 / (1.0 + _iter as f64 * 0.1);
            for j in 1..edge_data[i].control_points.len() - 1 {
                edge_data[i].control_points[j] =
                    edge_data[i].control_points[j].add(total_force.scale(move_factor));
            }
        }
    }

    // Extract final control points
    let bundled_edges: Vec<Vec<Point>> =
        edge_data.into_iter().map(|ed| ed.control_points).collect();

    BundlingResult {
        bundled_edges,
        iterations_completed: config.iterations,
        edge_count,
    }
}

/// Wire the bundling module into visual/mod.rs as `bundling_bundled`.
///
/// This function provides a high-level interface for CLI access.
pub fn bundling_bundled(
    edges: &[Edge],
    nodes: &[Node],
    control_points: Option<usize>,
    iterations: Option<usize>,
) -> BundlingResult {
    let mut config = BundlingConfig::default();
    if let Some(cp) = control_points {
        config.control_points = cp;
    }
    if let Some(it) = iterations {
        config.iterations = it;
    }
    force_directed_edge_bundling(edges, nodes, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_new() {
        let p = Point::new(3.0, 4.0);
        assert_eq!(p.x, 3.0);
        assert_eq!(p.y, 4.0);
    }

    #[test]
    fn test_point_distance() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        assert!((p1.distance_to(p2) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_distance_squared() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        assert!((p1.distance_squared_to(p2) - 25.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_add() {
        let p1 = Point::new(1.0, 2.0);
        let p2 = Point::new(3.0, 4.0);
        let result = p1.add(p2);
        assert!((result.x - 4.0).abs() < 1e-10);
        assert!((result.y - 6.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_subtract() {
        let p1 = Point::new(3.0, 4.0);
        let p2 = Point::new(1.0, 2.0);
        let result = p1.subtract(p2);
        assert!((result.x - 2.0).abs() < 1e-10);
        assert!((result.y - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_scale() {
        let p = Point::new(2.0, 3.0);
        let result = p.scale(2.0);
        assert!((result.x - 4.0).abs() < 1e-10);
        assert!((result.y - 6.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_midpoint() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(4.0, 4.0);
        let mid = p1.midpoint(p2);
        assert!((mid.x - 2.0).abs() < 1e-10);
        assert!((mid.y - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_lerp() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(10.0, 10.0);
        let lerp_half = p1.lerp(p2, 0.5);
        assert!((lerp_half.x - 5.0).abs() < 1e-10);
        assert!((lerp_half.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_magnitude() {
        let p = Point::new(3.0, 4.0);
        assert!((p.magnitude() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_normalize() {
        let p = Point::new(3.0, 4.0);
        let norm = p.normalize();
        assert!((norm.magnitude() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_normalize_zero() {
        let p = Point::new(0.0, 0.0);
        let norm = p.normalize();
        assert!(norm.x.abs() < 1e-10);
        assert!(norm.y.abs() < 1e-10);
    }

    #[test]
    fn test_point_dot() {
        let p1 = Point::new(1.0, 2.0);
        let p2 = Point::new(3.0, 4.0);
        assert!((p1.dot(p2) - 11.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_cross() {
        let p1 = Point::new(1.0, 2.0);
        let p2 = Point::new(3.0, 4.0);
        // 1*4 - 2*3 = -2
        assert!((p1.cross(p2) - (-2.0)).abs() < 1e-10);
    }

    #[test]
    fn test_edge_data_new() {
        let source = Point::new(0.0, 0.0);
        let target = Point::new(10.0, 0.0);
        let edge = EdgeData::new(source, target, 10);
        assert_eq!(edge.control_points.len(), 11);
        assert_eq!(edge.length, 10.0);
    }

    #[test]
    fn test_edge_data_initialize_control_points() {
        let source = Point::new(0.0, 0.0);
        let target = Point::new(10.0, 0.0);
        let edge = EdgeData::new(source, target, 5);
        // 5 intervals = 6 points
        assert_eq!(edge.control_points.len(), 6);
        // Check first and last
        assert_eq!(edge.control_points[0], source);
        assert_eq!(edge.control_points[5], target);
    }

    #[test]
    fn test_compatibility_thresholds_default() {
        let thresholds = CompatibilityThresholds::default();
        assert!((thresholds.angular - 0.3).abs() < 1e-10);
        assert!((thresholds.scale - 0.5).abs() < 1e-10);
        assert!((thresholds.position - 0.5).abs() < 1e-10);
        assert!((thresholds.visibility - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_bundling_config_default() {
        let config = BundlingConfig::default();
        assert_eq!(config.control_points, 20);
        assert_eq!(config.iterations, 6);
    }

    #[test]
    fn test_force_directed_edge_bundling_empty() {
        let edges: [Edge; 0] = [];
        let nodes: [Node; 0] = [];
        let result = force_directed_edge_bundling(&edges, &nodes, BundlingConfig::default());
        assert!(result.bundled_edges.is_empty());
        assert_eq!(result.edge_count, 0);
    }

    #[test]
    fn test_force_directed_edge_bundling_single_edge() {
        let edges = &[Edge { from: 0, to: 1 }];
        let nodes = &[
            Node {
                position: Point::new(0.0, 0.0),
            },
            Node {
                position: Point::new(10.0, 0.0),
            },
        ];
        let result = force_directed_edge_bundling(edges, nodes, BundlingConfig::default());
        assert_eq!(result.bundled_edges.len(), 1);
        assert_eq!(result.iterations_completed, 6);
        assert_eq!(result.edge_count, 1);
    }

    #[test]
    fn test_force_directed_edge_bundling_two_parallel_edges() {
        let edges = &[Edge { from: 0, to: 1 }, Edge { from: 2, to: 3 }];
        let nodes = &[
            Node {
                position: Point::new(0.0, 0.0),
            },
            Node {
                position: Point::new(10.0, 0.0),
            },
            Node {
                position: Point::new(0.0, 1.0),
            },
            Node {
                position: Point::new(10.0, 1.0),
            },
        ];
        let result = force_directed_edge_bundling(edges, nodes, BundlingConfig::default());
        assert_eq!(result.bundled_edges.len(), 2);
        assert_eq!(result.iterations_completed, 6);
        assert_eq!(result.edge_count, 2);
    }

    #[test]
    fn test_force_directed_edge_bundling_custom_config() {
        let edges = &[Edge { from: 0, to: 1 }];
        let nodes = &[
            Node {
                position: Point::new(0.0, 0.0),
            },
            Node {
                position: Point::new(10.0, 0.0),
            },
        ];
        let config = BundlingConfig {
            control_points: 5,
            iterations: 3,
            compatibility_thresholds: CompatibilityThresholds::default(),
        };
        let result = force_directed_edge_bundling(edges, nodes, config);
        assert_eq!(result.bundled_edges[0].len(), 6); // 5 intervals = 6 points
        assert_eq!(result.iterations_completed, 3);
    }

    #[test]
    fn test_bundling_bundled_helper() {
        let edges = &[Edge { from: 0, to: 1 }];
        let nodes = &[
            Node {
                position: Point::new(0.0, 0.0),
            },
            Node {
                position: Point::new(10.0, 0.0),
            },
        ];
        let result = bundling_bundled(edges, nodes, Some(10), Some(3));
        assert_eq!(result.bundled_edges[0].len(), 11); // 10 intervals = 11 points
        assert_eq!(result.iterations_completed, 3);
    }

    #[test]
    fn test_bundling_bundled_helper_defaults() {
        let edges = &[Edge { from: 0, to: 1 }];
        let nodes = &[
            Node {
                position: Point::new(0.0, 0.0),
            },
            Node {
                position: Point::new(10.0, 0.0),
            },
        ];
        let result = bundling_bundled(edges, nodes, None, None);
        assert_eq!(result.bundled_edges[0].len(), 21); // 20 intervals = 21 points
        assert_eq!(result.iterations_completed, 6);
    }

    #[test]
    fn test_compatibility_score_is_compatible() {
        let score = CompatibilityScore {
            angular: 0.5,
            scale: 0.6,
            position: 0.7,
            visibility: 0.8,
            combined: 0.65,
        };
        let thresholds = CompatibilityThresholds::default();
        assert!(score.is_compatible(&thresholds));
    }

    #[test]
    fn test_compatibility_score_not_compatible() {
        let score = CompatibilityScore {
            angular: 0.1, // below 0.3
            scale: 0.6,
            position: 0.7,
            visibility: 0.8,
            combined: 0.35,
        };
        let thresholds = CompatibilityThresholds::default();
        assert!(!score.is_compatible(&thresholds));
    }

    #[test]
    fn test_edge_serialize_roundtrip() {
        let edge = Edge { from: 1, to: 2 };
        let json = serde_json::to_string(&edge).unwrap();
        let restored: Edge = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.from, edge.from);
        assert_eq!(restored.to, edge.to);
    }

    #[test]
    fn test_node_serialize_roundtrip() {
        let node = Node {
            position: Point::new(1.5, 2.5),
        };
        let json = serde_json::to_string(&node).unwrap();
        let restored: Node = serde_json::from_str(&json).unwrap();
        assert!((restored.position.x - 1.5).abs() < 1e-10);
        assert!((restored.position.y - 2.5).abs() < 1e-10);
    }

    #[test]
    fn test_bundling_result_serialize_roundtrip() {
        let result = BundlingResult {
            bundled_edges: vec![
                vec![
                    Point::new(0.0, 0.0),
                    Point::new(5.0, 0.0),
                    Point::new(10.0, 0.0),
                ],
                vec![
                    Point::new(0.0, 1.0),
                    Point::new(5.0, 1.0),
                    Point::new(10.0, 1.0),
                ],
            ],
            iterations_completed: 6,
            edge_count: 2,
        };
        let json = serde_json::to_string(&result).unwrap();
        let restored: BundlingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.bundled_edges.len(), 2);
        assert_eq!(restored.iterations_completed, 6);
        assert_eq!(restored.edge_count, 2);
    }

    #[test]
    fn test_point_serialize_roundtrip() {
        let point = Point::new(3.14, 2.71);
        let json = serde_json::to_string(&point).unwrap();
        let restored: Point = serde_json::from_str(&json).unwrap();
        assert!((restored.x - 3.14).abs() < 1e-10);
        assert!((restored.y - 2.71).abs() < 1e-10);
    }
}

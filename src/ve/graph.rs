// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The cuboctahedron derived from coordinates rather than typed in.
//!
//! Vertices are the twelve points with one zero coordinate and two `±1`
//! coordinates, sorted lexicographically. Edges join vertices at squared
//! distance two. Triangular faces are the three-cliques. A proper four-colouring
//! of the edges, found by exhaustive search, partitions them into four perfect
//! matchings. Each edge lies in exactly one triangle, which serves as its
//! control face.

/// Number of vertices.
pub const VERTICES: usize = 12;
/// Number of edges.
pub const EDGES: usize = 24;
/// Number of triangular faces.
pub const TRIANGLES: usize = 8;
/// Number of perfect matchings in the edge partition.
pub const MATCHINGS: usize = 4;

/// Derived cuboctahedral structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cuboctahedron {
    /// Vertex coordinates in `{-1, 0, 1}^3`.
    pub vertices: [[i8; 3]; VERTICES],
    /// Edge endpoints `(a, b)` with `a < b`, sorted.
    pub edges: [(u8, u8); EDGES],
    /// Triangle vertex triples, sorted.
    pub triangles: [[u8; 3]; TRIANGLES],
    /// Matching colour of each edge in `0..4`.
    pub matching: [u8; EDGES],
    /// The unique triangle containing each edge.
    pub control_face: [u8; EDGES],
}

impl Cuboctahedron {
    /// Derive the structure. Cheap; call once and reuse.
    pub fn derive() -> Self {
        let mut vertices = Vec::with_capacity(VERTICES);
        for a in [-1_i8, 0, 1] {
            for b in [-1_i8, 0, 1] {
                for c in [-1_i8, 0, 1] {
                    let zeros = [a, b, c].iter().filter(|&&v| v == 0).count();
                    if zeros == 1 {
                        vertices.push([a, b, c]);
                    }
                }
            }
        }
        vertices.sort();
        let vertices: [[i8; 3]; VERTICES] = vertices.try_into().expect("twelve vertices");

        let mut edges = Vec::with_capacity(EDGES);
        for a in 0..VERTICES {
            for b in (a + 1)..VERTICES {
                if squared_distance(vertices[a], vertices[b]) == 2 {
                    edges.push((a as u8, b as u8));
                }
            }
        }
        let edges: [(u8, u8); EDGES] = edges.try_into().expect("twenty-four edges");

        let adjacent = |a: u8, b: u8| edges.contains(&(a.min(b), a.max(b)));
        let mut triangles = Vec::with_capacity(TRIANGLES);
        for a in 0..VERTICES as u8 {
            for b in (a + 1)..VERTICES as u8 {
                for c in (b + 1)..VERTICES as u8 {
                    if adjacent(a, b) && adjacent(b, c) && adjacent(a, c) {
                        triangles.push([a, b, c]);
                    }
                }
            }
        }
        let triangles: [[u8; 3]; TRIANGLES] = triangles.try_into().expect("eight triangles");

        let mut control_face = [u8::MAX; EDGES];
        for (edge_index, &(a, b)) in edges.iter().enumerate() {
            let owners: Vec<u8> = triangles
                .iter()
                .enumerate()
                .filter(|(_, tri)| tri.contains(&a) && tri.contains(&b))
                .map(|(index, _)| index as u8)
                .collect();
            assert_eq!(owners.len(), 1, "each edge lies in exactly one triangle");
            control_face[edge_index] = owners[0];
        }

        let mut matching = [u8::MAX; EDGES];
        let mut used = [[false; MATCHINGS]; VERTICES];
        assert!(
            colour_edges(&edges, 0, &mut matching, &mut used),
            "a one-factorisation exists"
        );

        Self {
            vertices,
            edges,
            triangles,
            matching,
            control_face,
        }
    }

    /// Edge indices belonging to one matching colour, in edge order.
    pub fn matching_edges(&self, colour: u8) -> impl Iterator<Item = usize> + '_ {
        self.matching
            .iter()
            .enumerate()
            .filter(move |(_, &c)| c == colour)
            .map(|(index, _)| index)
    }
}

fn squared_distance(a: [i8; 3], b: [i8; 3]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(&x, y)| {
            let d = i32::from(x) - i32::from(y);
            d * d
        })
        .sum()
}

fn colour_edges(
    edges: &[(u8, u8); EDGES],
    index: usize,
    matching: &mut [u8; EDGES],
    used: &mut [[bool; MATCHINGS]; VERTICES],
) -> bool {
    if index == EDGES {
        return true;
    }
    let (a, b) = edges[index];
    for colour in 0..MATCHINGS {
        if used[a as usize][colour] || used[b as usize][colour] {
            continue;
        }
        used[a as usize][colour] = true;
        used[b as usize][colour] = true;
        matching[index] = colour as u8;
        if colour_edges(edges, index + 1, matching, used) {
            return true;
        }
        used[a as usize][colour] = false;
        used[b as usize][colour] = false;
        matching[index] = u8::MAX;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_has_cuboctahedral_counts() {
        let g = Cuboctahedron::derive();
        assert_eq!(g.vertices.len(), 12);
        assert_eq!(g.edges.len(), 24);
        assert_eq!(g.triangles.len(), 8);
        for v in 0..VERTICES as u8 {
            let degree = g.edges.iter().filter(|(a, b)| *a == v || *b == v).count();
            assert_eq!(degree, 4);
        }
    }

    #[test]
    fn matchings_are_perfect_and_disjoint() {
        let g = Cuboctahedron::derive();
        for colour in 0..MATCHINGS as u8 {
            let edges: Vec<usize> = g.matching_edges(colour).collect();
            assert_eq!(edges.len(), 6);
            let mut touched = [0u8; VERTICES];
            for e in edges {
                let (a, b) = g.edges[e];
                touched[a as usize] += 1;
                touched[b as usize] += 1;
            }
            assert!(touched.iter().all(|&t| t == 1));
        }
    }

    #[test]
    fn every_edge_has_exactly_one_control_face() {
        let g = Cuboctahedron::derive();
        for (e, &(a, b)) in g.edges.iter().enumerate() {
            let tri = g.triangles[g.control_face[e] as usize];
            assert!(tri.contains(&a) && tri.contains(&b));
        }
        let mut covered = 0;
        for tri in g.triangles {
            for pair in [(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
                assert!(g.edges.contains(&pair));
                covered += 1;
            }
        }
        assert_eq!(covered, EDGES);
    }

    #[test]
    fn derivation_is_deterministic() {
        assert_eq!(Cuboctahedron::derive(), Cuboctahedron::derive());
    }
}

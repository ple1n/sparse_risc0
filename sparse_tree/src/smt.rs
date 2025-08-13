// This file is adapted from Webb and Arkworks:
// https://github.com/webb-tools/arkworks-gadgets

// Copyright (C) 2021 Webb Technologies Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) zkMove Authors
// SPDX-License-Identifier: Apache-2.0

//! This file provides a native implementation of the Sparse Merkle tree data
//! structure.
//!
//! A Sparse Merkle tree is a type of Merkle tree, but it is much easier to
//! prove non-membership in a sparse Merkle tree than in an arbitrary Merkle
//! tree. For an explanation of sparse Merkle trees, see:
//! `<https://medium.com/@kelvinfichter/whats-a-sparse-merkle-tree-acda70aeb837>`
//!
//! In this file we define the `Path` and `SparseMerkleTree` structs.
//! These depend on your choice of a prime field F, a field hasher over F
//! (any hash function that maps F^2 to F will do, e.g. the poseidon hash
//! function of width 3 where an input of zero is used for padding), and the
//! height N of the sparse Merkle tree.
//!
//! The path corresponding to a given leaf node is stored as an N-tuple of pairs
//! of field elements. Each pair consists of a node lying on the path from the
//! leaf node to the root, and that node's sibling.  For example, suppose
//! ```text
//!           a
//!         /   \
//!        b     c
//!       / \   / \
//!      d   e f   g
//! ```
//! is our Sparse Merkle tree, and `a` through `g` are field elements stored at
//! the nodes. Then the merkle proof path `e-b-a` from leaf `e` to root `a` is
//! stored as `[(d,e), (b,c)]`

#![allow(clippy::clone_on_copy)]

use anyhow::{bail, Error, Result};
use ark_std::Zero;
use digest::{consts::U256, Digest};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    borrow::ToOwned,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    io::Read,
    marker::PhantomData,
    ops::{Add, AddAssign},
};

/// Error enum for Sparse Merkle Tree.
#[derive(Debug)]
pub enum MerkleError {
    /// Thrown when the given leaf is not in the tree or the path.
    InvalidLeaf,
    /// Thrown when the merkle path is invalid.
    InvalidPathNodes,
}

impl core::fmt::Display for MerkleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            MerkleError::InvalidLeaf => "Invalid leaf".to_owned(),
            MerkleError::InvalidPathNodes => "Path nodes are not consistent".to_owned(),
        };
        write!(f, "{}", msg)
    }
}

impl std::error::Error for MerkleError {}

pub trait FieldExt: Clone + Eq + Copy + ToOwned<Owned = Self> + Serialize + Default {}
pub trait FieldHasher<F, const W: usize> {
    fn hash(&self, nodes: [F; W]) -> Result<F>;
}

/// The Path struct.
///
/// The path contains a sequence of sibling nodes that make up a merkle proof.
/// Each pair is used to identify whether an incremental merkle root
/// construction is valid at each intermediate step.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Path<F: FieldExt, const N: usize> {
    /// The path represented as a sequence of sibling pairs.
    pub path: heapless::Vec<(F, F), N>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Proof<F: FieldExt, const N: usize> {
    pub path: Path<F, N>,
    pub root: F,
    pub leaf: F,
}

impl<F: FieldExt + Serialize + DeserializeOwned, const N: usize> Proof<F, N>
where
    [(F, F); N]: DeserializeOwned + Serialize,
{
    pub fn verify<H: FieldHasher<F, 2>>(&self, h: &H) -> Result<bool> {
        self.path.check_membership(&self.root, &self.leaf, h)
    }
}

impl<F: FieldExt + Serialize + DeserializeOwned, const N: usize> Path<F, N>
where
    [(F, F); N]: DeserializeOwned + Serialize,
{
    /// Takes in an expected `root_hash` and leaf-level data (i.e. hashes of
    /// secrets) for a leaf and checks that the leaf belongs to a tree having
    /// the expected hash.
    pub fn check_membership<H: FieldHasher<F, 2>>(
        &self,
        root_hash: &F,
        leaf: &F,
        hasher: &H,
    ) -> Result<bool, Error> {
        let root = self.calculate_root(leaf, hasher)?;
        Ok(root == *root_hash)
    }

    /// Assumes leaf contains leaf-level data, i.e. hashes of secrets
    /// stored on leaf-level.
    pub fn calculate_root<H: FieldHasher<F, 2>>(&self, leaf: &F, hasher: &H) -> Result<F, Error> {
        if *leaf != self.path[0].0 && *leaf != self.path[0].1 {
            return Err(MerkleError::InvalidLeaf.into());
        }

        let mut prev = leaf.clone();
        // Check levels between leaf level and root
        for &(ref left_hash, ref right_hash) in &self.path {
            if &prev != left_hash && &prev != right_hash {
                return Err(MerkleError::InvalidPathNodes.into());
            }
            prev = hasher.hash([left_hash.clone(), right_hash.clone()])?;
        }

        Ok(prev)
    }
}

/// The Sparse Merkle Tree struct.
///
/// The Sparse Merkle Tree stores a set of leaves represented in a map and
/// a set of empty hashes that it uses to represent the sparse areas of the
/// tree.
pub struct SparseMerkleTree<F: FieldExt, H: FieldHasher<F, 2>, const N: usize> {
    /// A map from leaf indices to leaf data stored as field elements.
    pub tree: BTreeMap<u64, F>,
    /// An array of default hashes hashed with themselves `N` times.
    empty_hashes: heapless::Vec<F, N>,
    /// The phantom hasher type used to build the merkle tree.
    marker: PhantomData<H>,
}

impl<F: FieldExt, H: FieldHasher<F, 2>, const N: usize> SparseMerkleTree<F, H, N> {
    /// Takes a batch of field elements, inserts
    /// these hashes into the tree, and updates the merkle root.
    pub fn insert_batch(&mut self, leaves: &BTreeMap<u32, F>, hasher: &H) -> Result<(), Error> {
        let last_level_index: u64 = (1u64 << N) - 1;
        let mut level_idxs: BTreeSet<u64> = BTreeSet::new();
        for (i, leaf) in leaves {
            let true_index = last_level_index + (*i as u64);
            self.tree.insert(true_index, leaf.clone());
            let idx = parent(true_index);
            if let Some(idx) = idx {
                level_idxs.insert(idx);
            } else {
                bail!("parent not found");
            }
        }

        for level in 0..N {
            let mut new_idxs: BTreeSet<u64> = BTreeSet::new();
            let empty_hash = self.empty_hashes[level].clone();
            for i in level_idxs {
                let left_index = left_child(i);
                let right_index = right_child(i);
                let left = self.tree.get(&left_index).unwrap_or(&empty_hash);
                let right = self.tree.get(&right_index).unwrap_or(&empty_hash);
                self.tree
                    .insert(i, hasher.hash([left.clone(), right.clone()])?);

                let parent = match parent(i) {
                    Some(i) => i,
                    None => break,
                };
                new_idxs.insert(parent);
            }
            level_idxs = new_idxs;
        }

        Ok(())
    }

    /// Creates a new Sparse Merkle Tree from a map of indices to field
    /// elements.
    pub fn new(leaves: &BTreeMap<u32, F>, hasher: &H, empty_leaf: F) -> Result<Self, Error> {
        // Ensure the tree can hold this many leaves
        let last_level_size = leaves.len().next_power_of_two();
        let tree_size = 2 * last_level_size - 1;
        let tree_height = tree_height(tree_size as u64);
        assert!(tree_height <= N as u32);

        // Initialize the merkle tree
        let tree: BTreeMap<u64, F> = BTreeMap::new();
        let empty_hashes = gen_empty_hashes(hasher, empty_leaf)?;

        let mut smt = SparseMerkleTree::<F, H, N> {
            tree,
            empty_hashes,
            marker: PhantomData,
        };
        smt.insert_batch(leaves, hasher)?;

        Ok(smt)
    }

    /// Creates a new Sparse Merkle Tree from an array of field elements.
    pub fn new_sequential(leaves: &[F], hasher: &H, empty_leaf: F) -> Result<Self, Error> {
        let pairs: BTreeMap<u32, F> = leaves
            .iter()
            .enumerate()
            .map(|(i, l)| (i as u32, l.clone()))
            .collect();
        let smt = Self::new(&pairs, hasher, empty_leaf)?;

        Ok(smt)
    }

    /// Returns the Merkle tree root.
    pub fn root(&self) -> F {
        self.tree
            .get(&0)
            .cloned()
            .unwrap_or(self.empty_hashes.last().unwrap().clone())
    }

    /// Give the path leading from the leaf at `index` up to the root.  This is
    /// a "proof" in the sense of "valid path in a Merkle tree", not a ZK
    /// argument.
    pub fn generate_membership_path(&self, index: u64) -> Path<F, N> {
        let mut path = heapless::Vec::new();

        let tree_index = convert_index_to_last_level(index, N);

        // Iterate from the leaf up to the root, storing all intermediate hash values.
        let mut current_node = tree_index;
        let mut level = 0;
        while !is_root(current_node) {
            let sibling_node = sibling(current_node).unwrap();

            let empty_hash = &self.empty_hashes[level];

            let current = self.tree.get(&current_node).cloned().unwrap_or(*empty_hash);
            let sibling = self.tree.get(&sibling_node).cloned().unwrap_or(*empty_hash);

            if is_left_child(current_node) {
                path[level] = (current, sibling);
            } else {
                path[level] = (sibling, current);
            }
            current_node = parent(current_node).unwrap();
            level += 1;
        }

        Path { path }
    }

    pub fn generate_membership_proof(&self, index: u64) -> Proof<F, N> {
        let empty_hash = &self.empty_hashes[0];
        let tree_index = convert_index_to_last_level(index, N);

        Proof {
            path: self.generate_membership_path(index),
            root: self.root(),
            leaf: self.tree.get(&tree_index).unwrap_or(empty_hash).to_owned(),
        }
    }

    /// Leaves as in leaf in index in the leaf vector
    pub fn batch_prove(&self, leaves: &[u64]) -> PartialTree<F, N> {
        let mut partial = PartialTree {
            empty_hashes: self.empty_hashes.to_owned(),
            root: self.root(),
            ..Default::default()
        };

        for leaf in leaves {
            partial.leaves.push(*leaf);

            let tree_index = convert_index_to_last_level(*leaf, N);

            // Iterate from the leaf up to the root, storing all intermediate hash values.
            let mut current_node = tree_index;
            let mut level = 0;

            while !is_root(current_node) {
                let sibling_node = sibling(current_node).unwrap();

                let empty_hash = &self.empty_hashes[level];

                let current = self.tree.get(&current_node).cloned().unwrap_or(*empty_hash);
                let sibling = self.tree.get(&sibling_node).cloned().unwrap_or(*empty_hash);

                if current != *empty_hash {
                    partial.tree.insert(current_node, current);
                }
                if sibling != *empty_hash {
                    partial.tree.insert(sibling_node, sibling);
                }

                current_node = parent(current_node).unwrap();
                level += 1;
            }
        }

        partial
    }
}

// Partial tree
// Turn Vec<Path> Into a partial tree. Verify tree.

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct PartialTree<F: FieldExt, const N: usize> {
    pub tree: BTreeMap<u64, F>,
    empty_hashes: heapless::Vec<F, N>,
    /// as in map index. not tree index
    pub leaves: Vec<u64>,
    pub root: F,
}

impl<F: FieldExt + Debug, const N: usize> PartialTree<F, N> {
    pub fn verify<H: FieldHasher<F, 2>>(&self, hasher: &H) -> anyhow::Result<()> where {
        #[cfg(not(feature = "notzk"))]
        {
            use risc0_zkvm::guest::env;
            env::commit(&self.root);
            env::commit(&self.leaves);
            env::log("commited partial tree");
        }

        #[cfg(feature = "notzk")]
        {
            println!(
                "Tree proof, total elements {}, leaves {}",
                self.tree.len(),
                self.leaves.len()
            )
        }
        let last_level_index: u64 = (1u64 << N) - 1;
        let mut level_idxs: BTreeSet<u64> = BTreeSet::new();
        for i in &self.leaves {
            let true_index = last_level_index + *i;
            let idx = parent(true_index);
            if let Some(idx) = idx {
                level_idxs.insert(idx);
            } else {
                bail!("parent not found");
            }
        }

        for level in 0..(N - 1) {
            let mut new_idxs: BTreeSet<u64> = BTreeSet::new();
            let empty_hash_parent = self.empty_hashes[level + 1].clone();
            let empty_hash = self.empty_hashes[level].clone();
            // Each layer is only calculated once
            for i in level_idxs {
                let left_index = left_child(i);
                let right_index = right_child(i);
                let left = self.tree.get(&left_index).unwrap_or(&empty_hash);
                let right = self.tree.get(&right_index).unwrap_or(&empty_hash);

                let got = *self.tree.get(&i).unwrap_or(&empty_hash_parent);
                let expected = hasher.hash([left.clone(), right.clone()])?;
                assert!(expected == got);

                let parent = match parent(i) {
                    Some(i) => i,
                    None => break,
                };
                new_idxs.insert(parent);
            }
            level_idxs = new_idxs;
        }

        Ok(())
    }
}

/// A function to generate empty hashes with a given `default_leaf`.
///
/// Given a `FieldHasher`, generate a list of `N` hashes consisting
/// of the `default_leaf` hashed with itself and repeated `N` times
/// with the intermediate results. These are used to initialize the
/// sparse portion of the Sparse Merkle Tree.
pub fn gen_empty_hashes<F: FieldExt, H: FieldHasher<F, 2>, const N: usize>(
    hasher: &H,
    mut default_leaf: F,
) -> Result<heapless::Vec<F, N>, Error> {
    let mut empty_hashes = heapless::Vec::new();
    let mut item;
    for ix in 0..N {
        item = default_leaf;
        let _ = empty_hashes.push(item);
        default_leaf = hasher.hash([default_leaf, default_leaf])?;
    }
    assert!(empty_hashes.len() == N);

    Ok(empty_hashes)
}

fn convert_index_to_last_level(index: u64, height: usize) -> u64 {
    index + (1u64 << height) - 1
}

/// Returns the log2 value of the given number.
#[inline]
fn log2(number: u64) -> u32 {
    ark_std::log2(number as usize)
}

/// Returns the height of the tree, given the size of the tree.
#[inline]
fn tree_height(tree_size: u64) -> u32 {
    log2(tree_size)
}

/// Returns true iff the index represents the root.
#[inline]
fn is_root(index: u64) -> bool {
    index == 0
}

/// Returns the index of the left child, given an index.
#[inline]
fn left_child(index: u64) -> u64 {
    2 * index + 1
}

/// Returns the index of the right child, given an index.
#[inline]
fn right_child(index: u64) -> u64 {
    2 * index + 2
}

/// Returns the index of the sibling, given an index.
#[inline]
fn sibling(index: u64) -> Option<u64> {
    if index == 0 {
        None
    } else if is_left_child(index) {
        Some(index + 1)
    } else {
        Some(index - 1)
    }
}

/// Returns true iff the given index represents a left child.
#[inline]
fn is_left_child(index: u64) -> bool {
    index % 2 == 1
}

/// Returns the index of the parent, given an index.
#[inline]
fn parent(index: u64) -> Option<u64> {
    if index > 0 {
        Some((index - 1) >> 1)
    } else {
        None
    }
}

use sha2::digest::Update;
use sha2::Sha256;

pub type BYTE32 = [u8; 32];

impl FieldExt for [u8; 32] {}

impl<const N: usize> FieldHasher<[u8; 32], N> for Sha256 {
    fn hash(&self, nodes: [[u8; 32]; N]) -> Result<[u8; 32]> {
        let mut h = Sha256::new();
        for n in nodes {
            Update::update(&mut h, &n);
        }
        let f = h.finalize().to_vec();
        let mut s32 = [0; 32];
        s32.copy_from_slice(&f);
        Ok(s32)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::rngs::OsRng;
    use sha2::Sha256;
    use std::collections::BTreeMap;

    #[test]
    fn merkleput() {
        let mut leaves = vec![];
        for n in 0..10 {
            let mut s = [0; 32];
            s[0] = n;
            leaves.push(s);
        }
        let h = Sha256::new();
        let mut tree: SparseMerkleTree<[u8; 32], Sha256, 32> =
            SparseMerkleTree::new_sequential(&leaves, &h, [0; 32]).unwrap();
        let mut l1 = [0; 32];
        l1[0] = 222;

        let mut map = BTreeMap::new();
        map.insert(2, l1);
        let p = tree.insert_batch(&map, &h);
        let p = tree.generate_membership_path(5);
        dbg!(&p);
    }
}

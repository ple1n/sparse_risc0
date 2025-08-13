use serde::{Deserialize, Serialize};

use crate::smt::{PartialTree, BYTE32};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofClaims {
    pub root: BYTE32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProvingInput {
    pub pt: PartialTree<BYTE32, 32>,
    pub claim: ProofClaims
}
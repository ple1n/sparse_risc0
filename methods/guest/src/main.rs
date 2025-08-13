use risc0_zkvm::guest::{self, env};
use sha2::{Digest, Sha256};
use sparse_tree::{protocol::{ProofClaims, ProvingInput}, smt::{PartialTree, BYTE32}};

fn main() {
    let p1: ProvingInput = env::read();
    let h = Sha256::new();
    p1.pt.verify(&h).expect("fail");   
    env::commit(&p1.claim);
}
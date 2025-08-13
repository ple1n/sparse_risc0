use risc0_zkvm::guest::{self, env};
use sha2::{Digest, Sha256};
use sparse_tree::smt::{PartialTree, BYTE32};

fn main() {
    let pt: PartialTree<BYTE32, 32> = env::read();
    let h = Sha256::new();
    pt.verify(&h).expect("fail");
}
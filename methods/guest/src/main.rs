use num_bigint::BigInt;
use risc0_zkvm::guest::{self, env};
use sha2::Digest;
// use sparse_tree::smt::{MerkleProof, SMT};
use sparse_tree::ff::*;
use sparse_tree::{
    halo2_poseidon::Spec,
    halo2_proofs::pasta::Fp,
    poseidon::{FieldHasher, Poseidon, SmtP128Pow5T3},
};

fn main() {
    // let input: MerkleProof = env::read();
    // let smt = SMT::from_proof(true, input.clone());
    // let val = smt.verify_proof(input);
    // env::commit(&val);
    // bench_poseidon();
    bench_sha();
}

fn bench_poseidon() {
    let e: u64 = env::read();
    for x in 0..5 {
        let message = [Fp::from(e), Fp::from(x)];
        let (round_constants, mds, _) = SmtP128Pow5T3::<Fp, 0>::constants();

        let poseidon = Poseidon::<Fp, 2>::new();
        let result = poseidon.hash(message).unwrap();
        dbg!(&message, &result);
        let rx = result.to_repr();
        env::commit(&rx);
    }
}

fn bench_sha() {
    let e: u64 = env::read();
    for x in 0..5 {
        let message = [Fp::from(e), Fp::from(x)];

        let mut hasher = sha2::Sha256::new();
        for m in message{
            let r = m.to_repr();
            hasher.update(&r);
        }
        let rx = hasher.finalize().to_vec();
        env::commit(&rx);
    }
}

use ark_bn254::{Bn254, Fr, G1Affine, G2Affine, Fq, Fq2};
use ark_ff::PrimeField;
use ark_groth16::{Groth16, VerifyingKey, Proof};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_snark::SNARK;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use ark_ec::AffineRepr;

use serde::{Serialize, Deserialize};
use base64::{engine::general_purpose::STANDARD, Engine as _};

#[derive(Clone)]
struct SquareCircuit {
    pub x: Option<Fr>, // private input
    pub y: Option<Fr>, // public input
}

impl ConstraintSynthesizer<Fr> for SquareCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let x_var = FpVar::new_witness(cs.clone(), || {
            self.x.ok_or(SynthesisError::AssignmentMissing)
        })?;
        let y_var = FpVar::new_input(cs, || self.y.ok_or(SynthesisError::AssignmentMissing))?;
        let x_sq = &x_var * &x_var;
        x_sq.enforce_equal(&y_var)?;
        Ok(())
    }
}

/* ------------ Base64 JSON exports (compact) ------------ */

#[derive(Serialize, Deserialize)]
struct ProofJson { proof: String } // base64 (compressed)

#[derive(Serialize, Deserialize)]
struct VkJson { verifying_key: String } // base64 (compressed)

fn export_proof_json(proof: &Proof<Bn254>, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    proof.serialize_compressed(&mut bytes)?;
    let json = ProofJson { proof: STANDARD.encode(&bytes) };
    std::fs::write(path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

fn export_vk_json(vk: &VerifyingKey<Bn254>, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    vk.serialize_compressed(&mut bytes)?;
    let json = VkJson { verifying_key: STANDARD.encode(&bytes) };
    std::fs::write(path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

/* ------------ snarkjs-style VK export (human-readable coords) ------------ */
/* This matches the style you pasted (minus vk_alphabeta_12). */

#[derive(Serialize)]
struct SnarkJsVk {
    protocol: &'static str,    // "groth16"
    curve: &'static str,       // "bn128"
    nPublic: usize,
    vk_alpha_1: [String; 3],   // G1
    vk_beta_2: [[String; 2]; 3],   // G2
    vk_gamma_2: [[String; 2]; 3],  // G2
    vk_delta_2: [[String; 2]; 3],  // G2
    IC: Vec<[String; 3]>,      // G1 array, length = nPublic + 1
    // Note: snarkjs often also includes vk_alphabeta_12 (pairing precompute).
    // We omit it; verifiers can compute it when needed.
}

/* Helpers: convert field elements to decimal strings, and points to snarkjs arrays. */

fn fq_to_decimal(x: &Fq) -> String {
    // Canonical big integer (Montgomery form handled by ark-ff internally)
    x.into_bigint().to_string()
}

// snarkjs expects Fq2 as [c1, c0] (imaginary first), each as decimal string.
fn fq2_to_pair_snarkjs(x: &Fq2) -> [String; 2] {
    let (c0, c1) = (x.c0, x.c1);
    [fq_to_decimal(&c1), fq_to_decimal(&c0)]
}

fn g1_to_snarkjs(p: &G1Affine) -> [String; 3] {
    let p = p.into_group(); // ensure normalized
    let aff = G1Affine::from(p);
    [fq_to_decimal(&aff.x), fq_to_decimal(&aff.y), "1".to_string()]
}

fn g2_to_snarkjs(p: &G2Affine) -> [[String; 2]; 3] {
    let p = p.into_group();
    let aff = G2Affine::from(p);
    let x = fq2_to_pair_snarkjs(&aff.x);
    let y = fq2_to_pair_snarkjs(&aff.y);
    let z = ["1".to_string(), "0".to_string()];
    [x, y, z]
}

fn export_vk_snarkjs_json(vk: &VerifyingKey<Bn254>, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let n_public = vk.gamma_abc_g1.len() - 1;

    let mut ic: Vec<[String; 3]> = Vec::with_capacity(vk.gamma_abc_g1.len());
    for g in &vk.gamma_abc_g1 {
        ic.push(g1_to_snarkjs(g));
    }

    let out = SnarkJsVk {
        protocol: "groth16",
        curve: "bn128",
        nPublic: n_public,
        vk_alpha_1: g1_to_snarkjs(&vk.alpha_g1),
        vk_beta_2:  g2_to_snarkjs(&vk.beta_g2),
        vk_gamma_2: g2_to_snarkjs(&vk.gamma_g2),
        vk_delta_2: g2_to_snarkjs(&vk.delta_g2),
        IC: ic,
    };

    std::fs::write(path, serde_json::to_string_pretty(&out)?)?;
    Ok(())
}

/* ------------------------ main ------------------------ */

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // secret + public
    let x_val = Fr::from(7u64);
    let y_val = x_val * x_val; // 49

    // circuit
    let circuit = SquareCircuit { x: Some(x_val), y: Some(y_val) };

    // deterministic RNG for demo reproducibility
    let mut rng = StdRng::seed_from_u64(42);

    // setup
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit.clone(), &mut rng)?;

    // prove
    let proof = Groth16::<Bn254>::prove(&pk, circuit.clone(), &mut rng)?;
    println!("✅ Proof generated");

    // verify
    let is_valid = Groth16::<Bn254>::verify(&vk, &[y_val], &proof)?;
    println!("🔍 Verification result: {}", is_valid);

    // compact base64 JSON (previous approach)
    export_proof_json(&proof, "proof.json")?;
    export_vk_json(&vk, "vk.json")?;
    println!("💾 Saved compact proof.json and vk.json");

    // snarkjs-style VK JSON (what you pasted)
    export_vk_snarkjs_json(&vk, "vk_snarkjs.json")?;
    println!("💾 Saved snarkjs-style vk_snarkjs.json");

    Ok(())
}

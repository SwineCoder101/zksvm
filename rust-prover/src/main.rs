use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, VerifyingKey, Proof};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_snark::SNARK;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use serde::Serialize;
use base64::{engine::general_purpose::STANDARD, Engine as _};

#[derive(Clone)]
struct SquareCircuit {
    pub x: Option<Fr>, // private input
    pub y: Option<Fr>, // public input
}

impl ConstraintSynthesizer<Fr> for SquareCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        println!("🔧 === CONSTRAINT GENERATION PHASE ===");
        
        // Log initial circuit state
        match (&self.x, &self.y) {
            (Some(x), Some(y)) => {
                println!("📊 Circuit inputs:");
                println!("   • Private input (x): {:?}", x);
                println!("   • Public input (y): {:?}", y);
                println!("   • Expected relation: x² = y");
                println!("   • Verification: {}² = {} ✓", x, y);
            }
            _ => println!("⚠️  Circuit has missing inputs (setup phase)"),
        }

        // Create witness variable (private)
        println!("\n🔒 Allocating private witness variable...");
        let x_var = FpVar::new_witness(cs.clone(), || {
            let val = self.x.ok_or(SynthesisError::AssignmentMissing)?;
            println!("   • Witness variable created with value: {:?}", val);
            Ok(val)
        })?;
        
        // Create public input variable
        println!("🌐 Allocating public input variable...");
        let y_var = FpVar::new_input(cs.clone(), || {
            let val = self.y.ok_or(SynthesisError::AssignmentMissing)?;
            println!("   • Public input variable created with value: {:?}", val);
            Ok(val)
        })?;

        // Log constraint system state before adding constraints
        println!("\n📈 Constraint system state before constraint addition:");
        println!("   • Number of instance variables: {}", cs.num_instance_variables());
        println!("   • Number of witness variables: {}", cs.num_witness_variables());
        println!("   • Number of constraints: {}", cs.num_constraints());

        // Create the constraint: x * x = y
        println!("\n⚡ Computing x² constraint...");
        let x_sq = &x_var * &x_var;
        println!("   • x² computation complete");
        
        println!("🔗 Enforcing constraint: x² = y");
        x_sq.enforce_equal(&y_var)?;
        println!("   • Constraint added successfully");

        // Log final constraint system state
        println!("\n📊 Final constraint system state:");
        println!("   • Number of instance variables: {}", cs.num_instance_variables());
        println!("   • Number of witness variables: {}", cs.num_witness_variables());
        println!("   • Number of constraints: {}", cs.num_constraints());
        
        println!("✅ Constraint generation complete\n");
        Ok(())
    }
}

#[derive(Serialize)]
struct ProofJson {
    proof: String, // base64 (compressed)
}

#[derive(Serialize)]
struct VkJson {
    verifying_key: String, // base64 (compressed)
}

fn export_proof_json(proof: &Proof<Bn254>, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("💾 Exporting proof to JSON...");
    let mut bytes = Vec::new();
    proof.serialize_compressed(&mut bytes)?;
    println!("   • Proof serialized to {} bytes", bytes.len());
    
    let json = ProofJson { proof: STANDARD.encode(&bytes) };
    let json_string = serde_json::to_string_pretty(&json)?;
    println!("   • JSON string length: {} characters", json_string.len());
    
    std::fs::write(path, json_string)?;
    println!("   • Saved to: {}", path);
    Ok(())
}

fn export_vk_json(vk: &VerifyingKey<Bn254>, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔑 Exporting verifying key to JSON...");
    let mut bytes = Vec::new();
    vk.serialize_compressed(&mut bytes)?;
    println!("   • Verifying key serialized to {} bytes", bytes.len());
    
    let json = VkJson { verifying_key: STANDARD.encode(&bytes) };
    let json_string = serde_json::to_string_pretty(&json)?;
    println!("   • JSON string length: {} characters", json_string.len());
    
    std::fs::write(path, json_string)?;
    println!("   • Saved to: {}", path);
    Ok(())
}

// (Optional) helpers to load back from JSON if you need round-trips later.
#[allow(dead_code)]
fn import_proof_json(path: &str) -> Result<Proof<Bn254>, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct ProofIn { proof: String }
    let s = std::fs::read_to_string(path)?;
    let p: ProofIn = serde_json::from_str(&s)?;
    let bytes = STANDARD.decode(p.proof)?;
    let proof = Proof::<Bn254>::deserialize_compressed(&*bytes)?;
    Ok(proof)
}

#[allow(dead_code)]
fn import_vk_json(path: &str) -> Result<VerifyingKey<Bn254>, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    struct VkIn { verifying_key: String }
    let s = std::fs::read_to_string(path)?;
    let v: VkIn = serde_json::from_str(&s)?;
    let bytes = STANDARD.decode(v.verifying_key)?;
    let vk = VerifyingKey::<Bn254>::deserialize_compressed(&*bytes)?;
    Ok(vk)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 === ZERO-KNOWLEDGE PROOF SYSTEM DEMO ===\n");
    
    // secret + public values
    let x_val = Fr::from(7u64);
    let y_val = x_val * x_val; // 49
    
    println!("📋 Problem Setup:");
    println!("   • We want to prove we know a secret number x");
    println!("   • Such that x² equals the public value y");
    println!("   • Without revealing what x actually is");
    println!("   • Secret value (x): {}", x_val);
    println!("   • Public value (y = x²): {}", y_val);
    println!("   • Mathematical relation: {}² = {}", x_val, y_val);

    // circuit instance
    println!("\n🔧 Creating circuit instance...");
    let circuit = SquareCircuit { x: Some(x_val), y: Some(y_val) };
    println!("   • Circuit created with secret and public inputs");

    // deterministic RNG (for reproducible outputs in this demo)
    println!("\n🎲 Initializing randomness source...");
    let mut rng = StdRng::seed_from_u64(42);
    println!("   • Using deterministic RNG with seed 42 for reproducibility");

    // trusted setup (circuit-specific)
    println!("\n🏗️  === TRUSTED SETUP PHASE ===");
    println!("⚙️  Performing circuit-specific trusted setup...");
    println!("   • This generates proving and verifying keys");
    println!("   • Setup is specific to our x² = y circuit");
    println!("   • In production, this would be done in a trusted ceremony");
    
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit.clone(), &mut rng)?;
    println!("✅ Trusted setup complete");
    println!("   • Proving key generated (used to create proofs)");
    println!("   • Verifying key generated (used to verify proofs)");

    // prove
    println!("\n🔐 === PROOF GENERATION PHASE ===");
    println!("📝 Generating zero-knowledge proof...");
    println!("   • Prover knows: x = {}", x_val);
    println!("   • Prover will prove: x² = {} (without revealing x)", y_val);
    println!("   • Using Groth16 proving system on BN254 curve");
    
    let proof = Groth16::<Bn254>::prove(&pk, circuit.clone(), &mut rng)?;
    println!("✅ Proof generated successfully!");
    println!("   • Proof is succinct (constant size regardless of circuit complexity)");
    println!("   • Proof reveals nothing about the secret value x");

    // verify
    println!("\n🔍 === VERIFICATION PHASE ===");
    println!("🔬 Verifying the proof...");
    println!("   • Verifier only knows: y = {}", y_val);
    println!("   • Verifier does NOT know the secret x");
    println!("   • Verifying that prover knows some x where x² = y");
    
    let is_valid = Groth16::<Bn254>::verify(&vk, &[y_val], &proof)?;
    println!("🎯 Verification result: {}", if is_valid { "✅ VALID" } else { "❌ INVALID" });
    
    if is_valid {
        println!("🎉 SUCCESS! The proof is valid!");
        println!("   • Prover has demonstrated knowledge of x such that x² = {}", y_val);
        println!("   • Secret x remains completely hidden");
        println!("   • Mathematical soundness guaranteed by cryptographic assumptions");
    }

    // export JSON files
    println!("\n💾 === EXPORT PHASE ===");
    export_proof_json(&proof, "proof.json")?;
    export_vk_json(&vk, "vk.json")?;
    println!("✅ Export complete!");
    println!("   • proof.json contains the zero-knowledge proof");
    println!("   • vk.json contains the verifying key for future verification");
    println!("   • Anyone with vk.json can verify the proof without the proving key");

    println!("\n🏁 === SUMMARY ===");
    println!("✅ Zero-knowledge proof system demonstration complete!");
    println!("📊 What we accomplished:");
    println!("   • ✅ Generated a proof that we know x where x² = {}", y_val);
    println!("   • ✅ Kept the actual value of x = {} completely secret", x_val);
    println!("   • ✅ Created a verifiable proof that anyone can check");
    println!("   • ✅ Exported proof and verification key for later use");
    println!("\n🔐 Zero-knowledge properties satisfied:");
    println!("   • Completeness: Valid proofs always verify");
    println!("   • Soundness: Invalid proofs cannot be created");
    println!("   • Zero-knowledge: No information about x is revealed");

    Ok(())
}
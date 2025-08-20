# Zero-Knowledge Proof Demo 🔐

A minimal Rust implementation demonstrating zero-knowledge proofs using the Groth16 proving system on the BN254 elliptic curve. This example proves knowledge of a secret number `x` such that `x² = y` without revealing `x`.

## What This Demonstrates

**Zero-Knowledge Properties:**
- ✅ **Completeness**: Valid proofs always verify
- ✅ **Soundness**: Invalid proofs cannot be forged  
- ✅ **Zero-Knowledge**: No information about the secret is revealed

**Real-World Application**: This pattern is fundamental to many blockchain privacy solutions, identity systems, and confidential computations.

## Quick Start

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add to Cargo.toml
[dependencies]
ark-std = { version = "0.5", features = ["std"] }
ark-ff = "0.5"
ark-bn254 = "0.5"
ark-relations = "0.5"
ark-r1cs-std = "0.5"
ark-groth16 = "0.5"
ark-snark = "0.5"
rand = "0.9.2"
ark-serialize = "0.5"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
base64 = "0.22"
ark-ec = "0.5"
```

### Run the Demo

```bash
cargo run              # runs src/main.rs (default)
cargo run --bin snarkjs # runs src/snarkjs.rs
```

## How It Works

### 1. **Circuit Definition**
```rust
struct SquareCircuit {
    pub x: Option<Fr>, // private input (secret)
    pub y: Option<Fr>, // public input  
}
```

The circuit enforces the constraint: `x * x = y`

### 2. **Trusted Setup**
Generates cryptographic keys specific to our circuit:
- **Proving Key**: Used to generate proofs
- **Verifying Key**: Used to verify proofs (can be shared publicly)

### 3. **Proof Generation**
Creates a zero-knowledge proof that demonstrates knowledge of `x` without revealing it.

### 4. **Verification**
Anyone with the verifying key can confirm the proof is valid without learning the secret.

## Example Output

```
Secret x: 7, Public y: 49
Generating constraints...
Constraints: 1 variables, 1 constraints
Running trusted setup...
Setup complete
Generating proof...
✅ Proof generated
🔍 Verification result: true
Exported proof (192 bytes) to proof.json
Exported verifying key (573 bytes) to vk.json
```

## Generated Files

- **`proof.json`**: Contains the zero-knowledge proof (base64 encoded)
- **`vk.json`**: Contains the verifying key for future verification

## Technical Stack

- **Proving System**: Groth16 (efficient, constant-size proofs)
- **Elliptic Curve**: BN254 (pairing-friendly, Ethereum compatible)
- **Constraint System**: R1CS (Rank-1 Constraint Systems)
- **Library**: Arkworks (leading Rust cryptography framework)

## Key Concepts

### Circuit Constraints
The heart of any ZK system is the constraint system. Our simple example has one constraint:
```
x * x = y
```

More complex applications might have thousands of constraints representing:
- Hash function computations
- Merkle tree verifications  
- Business logic validation
- Mathematical proofs

### Trusted Setup
Groth16 requires a one-time trusted setup per circuit. In production:
- Multiple independent parties participate in a "ceremony"
- If ANY party is honest, the system remains secure
- Modern setups use techniques like "Powers of Tau" for scalability

### Proof Size & Speed
- **Proof Size**: ~200 bytes (constant, regardless of circuit complexity)
- **Verification**: Milliseconds (constant time)
- **Generation**: Varies with circuit size (seconds to minutes for complex circuits)

## Advanced Topics

### Circuit Optimization
- Minimize the number of constraints for faster proving
- Use lookup tables for complex operations
- Optimize field arithmetic

### Production Considerations
- Secure parameter generation ceremonies
- Proof caching and batching
- Integration with blockchain systems
- Privacy-preserving key management

## Contributing

This is a minimal educational example. For production use, consider:
- Input validation and error handling
- Secure randomness generation
- Circuit auditing and formal verification
- Performance optimization

## Resources

- 📚 [Arkworks Documentation](https://arkworks.rs/)
- 🔗 [ZK Learning Resources](https://zkp.science/)
- 📖 [Groth16 Paper](https://eprint.iacr.org/2016/260.pdf)
- 🛠️ [Circom](https://github.com/iden3/circom) - Alternative circuit language

## License

MIT License - Feel free to build upon this example!

---

*"Zero-knowledge proofs: proving you know something without revealing what you know."*
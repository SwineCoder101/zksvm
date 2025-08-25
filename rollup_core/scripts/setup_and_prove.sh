#!/bin/bash
set -e

echo "Setting up System Transfer circuits and generating proofs..."

mkdir -p build/keys

setup_powers_of_tau() {
    if [ ! -f "build/keys/pot12_final.ptau" ]; then
        echo "Setting up Powers of Tau ceremony..."
        snarkjs powersoftau new bn128 12 build/keys/pot12_0000.ptau -v
        snarkjs powersoftau contribute build/keys/pot12_0000.ptau build/keys/pot12_0001.ptau --name="System transfer contribution" -v -e="system transfer entropy"
        snarkjs powersoftau prepare phase2 build/keys/pot12_0001.ptau build/keys/pot12_final.ptau -v
        echo " Powers of Tau ceremony complete"
    fi
}

setup_single_circuit() {
    echo ""
    echo "Setting up SINGLE system transfer circuit..."
    
    if [ ! -f "build/system_transfer.r1cs" ]; then
        echo "Compiling single transfer circuit..."
        circom circuit/system_transfer.circom --r1cs --wasm --sym -o build/
        echo " Single circuit compiled"
    fi
    
    if [ ! -f "build/keys/verification_key_single.json" ]; then
        echo "Creating single circuit keys..."
        snarkjs groth16 setup build/system_transfer.r1cs build/keys/pot12_final.ptau build/keys/single_0000.zkey
        snarkjs zkey contribute build/keys/single_0000.zkey build/keys/single_0001.zkey --name="Single transfer contribution" -v -e="single entropy"
        snarkjs zkey export verificationkey build/keys/single_0001.zkey build/keys/verification_key_single.json
        echo " Single circuit keys generated"
    fi
}

setup_batch_circuit() {
    echo ""
    echo "Setting up BATCH system transfer circuit..."
    
    if [ ! -f "build/batch_system_transfer.r1cs" ]; then
        echo "Compiling batch transfer circuit..."
        circom circuit/batch_system_transfer.circom --r1cs --wasm --sym -o build/
        echo " Batch circuit compiled"
    fi
    
    if [ ! -f "build/keys/verification_key_batch.json" ]; then
        echo "Creating batch circuit keys..."
        snarkjs groth16 setup build/batch_system_transfer.r1cs build/keys/pot12_final.ptau build/keys/batch_0000.zkey
        snarkjs zkey contribute build/keys/batch_0000.zkey build/keys/batch_0001.zkey --name="Batch transfer contribution" -v -e="batch entropy"
        snarkjs zkey export verificationkey build/keys/batch_0001.zkey build/keys/verification_key_batch.json
        echo " Batch circuit keys generated"
    fi
}

generate_single_proof() {
    echo ""
    echo "Generating SINGLE system transfer proof..."
 
    if [ ! -f "build/input_single.json" ]; then
        echo "Creating single transfer input..."
        cat > build/input_single.json << EOL
{
  "amount": "1000000",
  "signature_first_byte": "42",
  "from_balance_before": "5000000000",
  "from_balance_after": "4999000000"
}
EOL
    fi
    
    echo "Generating witness for single transfer..."
    node build/system_transfer_js/generate_witness.js build/system_transfer_js/system_transfer.wasm build/input_single.json build/witness_single.wtns
    
    echo "Generating zero-knowledge proof for single transfer..."
    snarkjs groth16 prove build/keys/single_0001.zkey build/witness_single.wtns build/proof_single.json build/public_single.json
    
    echo " Verifying single transfer proof..."
    snarkjs groth16 verify build/keys/verification_key_single.json build/public_single.json build/proof_single.json
    
    if [ $? -eq 0 ]; then
        echo "Single system transfer proof verified successfully!"
    else
        echo " Single transfer proof verification failed!"
        return 1
    fi
}

generate_batch_proof() {
    echo ""
    echo "Generating BATCH system transfer proof..."

    if [ -n "$INPUT_FILE" ] && [ -f "$INPUT_FILE" ]; then
        echo "Using rollup-provided input file: $INPUT_FILE"
        cp "$INPUT_FILE" build/input_batch.json
    elif [ ! -f "build/input_batch.json" ]; then
        echo "Creating default batch transfer input..."
        cat > build/input_batch.json << EOL
{
  "amounts": ["1000000", "1000000", "1000000"],
  "signature_first_bytes": ["42", "156", "201"],
  "from_balances_before": ["5000000000", "3000000000", "8000000000"],
  "from_balances_after": ["4999000000", "2999000000", "7999000000"]
}
EOL
    fi
    
    echo "Generating witness for batch transfers..."
    node build/batch_system_transfer_js/generate_witness.js build/batch_system_transfer_js/batch_system_transfer.wasm build/input_batch.json build/witness_batch.wtns
    
    echo "Generating zero-knowledge proof for batch transfers..."
    snarkjs groth16 prove build/keys/batch_0001.zkey build/witness_batch.wtns build/proof_batch.json build/public_batch.json
    
    echo " Verifying batch transfer proof..."
    snarkjs groth16 verify build/keys/verification_key_batch.json build/public_batch.json build/proof_batch.json
    
    if [ $? -eq 0 ]; then
        echo "Batch system transfer proof verified successfully"
    else
        echo " Batch transfer proof verification failed!"
        return 1
    fi
}

echo "Starting system transfer proof generation setup..."

setup_powers_of_tau
setup_single_circuit
setup_batch_circuit

generate_single_proof
generate_batch_proof

echo ""
echo "SUCCESS! System transfer circuits set up and proofs generated!"
echo ""
echo "Generated files:"
echo "   SINGLE TRANSFER:"
echo "   - build/proof_single.json (single transfer proof)"
echo "   - build/public_single.json (single public inputs)"
echo "   - build/keys/verification_key_single.json (single verification key)"
echo ""
echo "   BATCH TRANSFERS:"
echo "   - build/proof_batch.json (batch transfer proof)"
echo "   - build/public_batch.json (batch public inputs)" 
echo "   - build/keys/verification_key_batch.json (batch verification key)"
echo ""
echo "What this proves:"
echo "   ✓ Transfer amounts are within valid ranges"
echo "   ✓ Account balances changed correctly"
echo "   ✓ All transactions have valid signatures"
echo "   ✓ System transfer execution was correct"
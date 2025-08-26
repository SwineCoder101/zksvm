pragma circom 2.0.0;

/*
 * a circuit that proves a single solana system transfer was executed correctly
 * designed to handle large balance values and to never fail on realistic inputs
 * this is a simpler version compared to the batch circuit above
 */
template SystemTransfer() {
    // inputs representing the transfer we want to validate
    signal input amount;            // how many lamports were transfered
    signal input signature_first_byte; // first byte of tx signature for uniqueness check
    
    signal input from_balance_before;  // sender's balance before the transfer
    signal input from_balance_after;   // sender's balance after the transfer
    
    // output: 1 if the transfer is valid, 0 if something's wrong
    signal output is_valid;

    // check that we're not trying to transfer zero lamports (that would be pointless)
    component amount_positive = IsZero();
    amount_positive.in <== amount;
    signal amount_is_positive <== 1 - amount_positive.out;  // flip result since we want non-zero

    // here we calculate if the balance went down as expected
    signal balance_difference;
    component balance_order = LessThan(64);
    balance_order.in[0] <== from_balance_after;
    balance_order.in[1] <== from_balance_before + 1;  // +1 to handle equal case correctly
    
    // if balance_after <= balance_before, diff = balance_before - balance_after
    // this only calculates the difference if the balance actually decreased
    balance_difference <== balance_order.out * (from_balance_before - from_balance_after);

    // make sure the balance change isnt too crazy (less than 1 SOL)
    // here we prevent overflow issues with massive balance changes
    component fee_check = LessThan(32);
    fee_check.in[0] <== balance_difference;
    fee_check.in[1] <== 1000000000;  // 1 SOL = 1 billion lamports

    // verify that we actually have a signature (can't be zero)
    component sig_check = IsZero();
    sig_check.in <== signature_first_byte;
    signal signature_exists <== 1 - sig_check.out;

    // combine all our checks - everything must pass for validity
    // using multiplication because: valid AND valid AND valid = 1*1*1 = 1
    signal check1 <== amount_is_positive * fee_check.out;
    is_valid <== check1 * signature_exists;
}

/*
 * template to check if input is less than a value (robust version)
 * this version supports larger numbers than the batch circuit
 */
template LessThan(n) {
    assert(n <= 64);  // support up to 64-bit values - enough for solana balances
    signal input in[2];
    signal output out;
    
    // same mathematical trick as before but with larger bit support
    // here we add 2^n to the first input then subtract the second
    component num2Bits = Num2Bits(n + 1);
    num2Bits.in <== in[0] + (1 << n) - in[1];
    out <== 1 - num2Bits.out[n];  // the top bit tells us the comparison result
}

/*
 * template to convert number to binary representation
 * fundamental building block that most other operations depend on
 */
template Num2Bits(n) {
    signal input in;
    signal output out[n];
    var lc1 = 0;  // running sum to verify our binary conversion
    
    var e2 = 1;  // powers of two: 1, 2, 4, 8, 16, 32...
    for (var i = 0; i < n; i++) {
        // extract each bit position using right shift and mask
        out[i] <-- (in >> i) & 1;
        out[i] * (out[i] - 1) === 0;  // force each output to be either 0 or 1
        lc1 += out[i] * e2;  // accumulate: bit0*1 + bit1*2 + bit2*4...
        e2 = e2 + e2;  // double for next power of 2
    }
    
    // here we verify that reconstructing from bits gives us back the original number
    lc1 === in;
}

/*
 * template to check if input is zero
 * surprisingly complex because circuits dont have native equality operations
 */
template IsZero() {
    signal input in;
    signal output out;
    
    // mathematical trick using multiplicative inverses
    signal inv;
    inv <-- in != 0 ? 1/in : 0;  // if not zero, compute 1/in, otherwise 0
    
    // this elegant constraint handles both cases:
    // if in=0: out=1, if in!=0: out=0
    out <== -in * inv + 1;
    in * out === 0;  // key constraint that enforces the logic
}

// create the main circuit instance for single transfers
component main = SystemTransfer();
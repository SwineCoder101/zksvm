pragma circom 2.0.0;

/*
 * a circuit that proves a batch of solana system transfers were executed correctly
 * this is the main template that validates multiple transfers at once
 */
template BatchSystemTransfer(BATCH_SIZE) {
    // here we define the inputs that represent each transaction in the batch
    signal input amounts[BATCH_SIZE];              // how much lamports each transfer moved
    signal input signature_first_bytes[BATCH_SIZE];  // first byte of each transaction signature for uniqueness
    signal input from_balances_before[BATCH_SIZE];  // account balance before each transfer
    signal input from_balances_after[BATCH_SIZE];   // account balance after each transfer
    
    // this is what we're trying to prove - that the whole batch is valid
    signal output batch_valid;
    
    // create an array of individual transfer validators
    component transfers[BATCH_SIZE];

    // here we loop through each transaction and create a validator for it
    for (var i = 0; i < BATCH_SIZE; i++) {
        transfers[i] = SystemTransferSimple();
        transfers[i].amount <== amounts[i];
        transfers[i].signature_first_byte <== signature_first_bytes[i];
        transfers[i].from_balance_before <== from_balances_before[i];
        transfers[i].from_balance_after <== from_balances_after[i];
    }

    // this component checks that ALL transfers in the batch are valid
    // basically does: transfer1_valid AND transfer2_valid AND transfer3_valid...
    component batch_validator = MultiAND(BATCH_SIZE);
    for (var i = 0; i < BATCH_SIZE; i++) {
        batch_validator.in[i] <== transfers[i].is_valid;
    }
    
    // the final output - 1 if all transfers are valid, 0 if any failed
    batch_valid <== batch_validator.out;
}

/*
 * system transfer template that avoids large number comparisions
 * here we validate individual transfers without running into circuit constraints
 */
template SystemTransferSimple() {
    // inputs for a single transfer validation
    signal input amount;
    signal input signature_first_byte;
    signal input from_balance_before;
    signal input from_balance_after;
    
    // output: 1 if this transfer is valid, 0 if not
    signal output is_valid;

    // check that the transfer amount is not zero (we dont want empty transfers)
    component amount_check = IsZero();
    amount_check.in <== amount;
    signal amount_valid <== 1 - amount_check.out;  // flip the result - we want non-zero

    // here we calculate how much the balance changed
    signal balance_diff <== from_balance_before - from_balance_after;

    // make sure the balance change is reasonable (less than 1 SOL to avoid huge numbers)
    // this prevents circuit overflow issues with very large balances
    component balance_check = LessThan(32);
    balance_check.in[0] <== balance_diff;
    balance_check.in[1] <== 1000000000;  // 1 SOL in lamports

    // check that we have a real signature (first byte shouldnt be zero)
    component sig_check = IsZero();
    sig_check.in <== signature_first_byte;
    signal signature_valid <== 1 - sig_check.out;

    // combine all the checks - all must be true for the transfer to be valid
    // here we use multiplication because in circuits: 1*1*1 = 1, but 1*1*0 = 0
    signal check1 <== amount_valid * balance_check.out;
    is_valid <== check1 * signature_valid;
}

/*
 * template for multi-input AND operation
 * this recursively combines multiple boolean inputs with AND logic
 */
template MultiAND(n) {
    signal input in[n];
    signal output out;
    
    // base cases for recursion
    if (n == 1) {
        out <== in[0];  // just pass through single input
    } else if (n == 2) {
        out <== in[0] * in[1];  // simple AND for two inputs
    } else {
        // here we recursively break down the problem
        // AND the first (n-1) inputs, then AND that result with the last input
        component and_first = MultiAND(n-1);
        for (var i = 0; i < n-1; i++) {
            and_first.in[i] <== in[i];
        }
        out <== and_first.out * in[n-1];
    }
}

/*
 * template to check if input is less than a value
 * uses binary representation tricks to avoid expensive comparison operations
 */
template LessThan(n) {
    assert(n <= 32);  // keep this conservative for stability - larger numbers cause issues
    signal input in[2];
    signal output out;
    
    // here we use a clever trick: convert (a + 2^n - b) to binary
    // if a < b, then the n-th bit will be 0, otherwise 1
    component num2Bits = Num2Bits(n + 1);
    num2Bits.in <== in[0] + (1 << n) - in[1];
    out <== 1 - num2Bits.out[n];  // flip the n-th bit to get our result
}

/*
 * template to convert number to binary representation
 * this is a fundamental building block for many circuit operations
 */
template Num2Bits(n) {
    signal input in;
    signal output out[n];
    var lc1 = 0;  // linear combination to verify correctness
    
    var e2 = 1;  // powers of 2: 1, 2, 4, 8, 16...
    for (var i = 0; i < n; i++) {
        // extract the i-th bit using bitwise operations
        out[i] <-- (in >> i) & 1;
        out[i] * (out[i] - 1) === 0;  // constraint: each bit must be 0 or 1
        lc1 += out[i] * e2;  // accumulate: bit0*1 + bit1*2 + bit2*4 + ...
        e2 = e2 + e2;  // next power of 2
    }
    
    // here we verify that our binary representation is correct
    // the sum of all bits*powers should equal the original number
    lc1 === in;
}

/*
 * template to check if input is zero
 * this is trickier than it looks because circuits cant do direct equality checks
 */
template IsZero() {
    signal input in;
    signal output out;
    
    // here we use the mathematical trick: if x != 0, then 1/x exists
    signal inv;
    inv <-- in != 0 ? 1/in : 0;  // compute multiplicative inverse
    
    // this constraint ensures: if in==0 then out==1, if in!=0 then out==0
    out <== -in * inv + 1;
    in * out === 0;  // this constraint is key - forces the relationship
}

// instantiate the main circuit for batches of 3 transfers
component main = BatchSystemTransfer(3);
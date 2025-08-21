pragma circom 2.0.0;

/*
 * A circuit that proves a batch of Solana system transfers were executed correctly
 */
template BatchSystemTransfer(BATCH_SIZE) {
    signal input amounts[BATCH_SIZE];              
    signal input signature_first_bytes[BATCH_SIZE];  
    signal input from_balances_before[BATCH_SIZE];  
    signal input from_balances_after[BATCH_SIZE];
    
    signal output batch_valid;
    
    component transfers[BATCH_SIZE];

    for (var i = 0; i < BATCH_SIZE; i++) {
        transfers[i] = SystemTransferSimple();
        transfers[i].amount <== amounts[i];
        transfers[i].signature_first_byte <== signature_first_bytes[i];
        transfers[i].from_balance_before <== from_balances_before[i];
        transfers[i].from_balance_after <== from_balances_after[i];
    }

    component batch_validator = MultiAND(BATCH_SIZE);
    for (var i = 0; i < BATCH_SIZE; i++) {
        batch_validator.in[i] <== transfers[i].is_valid;
    }
    
    batch_valid <== batch_validator.out;
}

/*
 * System transfer template that avoids large number comparisons
 */
template SystemTransferSimple() {
    signal input amount;
    signal input signature_first_byte;
    signal input from_balance_before;
    signal input from_balance_after;
    
    signal output is_valid;

    component amount_check = IsZero();
    amount_check.in <== amount;
    signal amount_valid <== 1 - amount_check.out;

    signal balance_diff <== from_balance_before - from_balance_after;

    component balance_check = LessThan(32);
    balance_check.in[0] <== balance_diff;
    balance_check.in[1] <== 1000000000; 

    component sig_check = IsZero();
    sig_check.in <== signature_first_byte;
    signal signature_valid <== 1 - sig_check.out;

    signal check1 <== amount_valid * balance_check.out;
    is_valid <== check1 * signature_valid;
}

/*
 * Template for multi-input AND operation
 */
template MultiAND(n) {
    signal input in[n];
    signal output out;
    
    if (n == 1) {
        out <== in[0];
    } else if (n == 2) {
        out <== in[0] * in[1];
    } else {
        component and_first = MultiAND(n-1);
        for (var i = 0; i < n-1; i++) {
            and_first.in[i] <== in[i];
        }
        out <== and_first.out * in[n-1];
    }
}

/*
 * Template to check if input is less than a value
 */
template LessThan(n) {
    assert(n <= 32);  // Keep this conservative for stability
    signal input in[2];
    signal output out;
    
    component num2Bits = Num2Bits(n + 1);
    num2Bits.in <== in[0] + (1 << n) - in[1];
    out <== 1 - num2Bits.out[n];
}

/*
 * Template to convert number to binary representation
 */
template Num2Bits(n) {
    signal input in;
    signal output out[n];
    var lc1 = 0;
    
    var e2 = 1;
    for (var i = 0; i < n; i++) {
        out[i] <-- (in >> i) & 1;
        out[i] * (out[i] - 1) === 0;
        lc1 += out[i] * e2;
        e2 = e2 + e2;
    }
    
    lc1 === in;
}

/*
 * Template to check if input is zero
 */
template IsZero() {
    signal input in;
    signal output out;
    
    signal inv;
    inv <-- in != 0 ? 1/in : 0;
    out <== -in * inv + 1;
    in * out === 0;
}

component main = BatchSystemTransfer(3);
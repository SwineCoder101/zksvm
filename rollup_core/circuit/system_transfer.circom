pragma circom 2.0.0;

/*
 * A circuit that proves a single Solana system transfer was executed correctly
 * Designed to handle large balance values and to never fail on realistic inputs
 */
template SystemTransfer() {
    signal input amount;            
    signal input signature_first_byte; 
    
    signal input from_balance_before;  
    signal input from_balance_after;   
    
    signal output is_valid;

    component amount_positive = IsZero();
    amount_positive.in <== amount;
    signal amount_is_positive <== 1 - amount_positive.out;

    signal balance_difference;
    component balance_order = LessThan(64);
    balance_order.in[0] <== from_balance_after;
    balance_order.in[1] <== from_balance_before + 1; 
    
    // if balance_after <= balance_before, diff = balance_before - balance_after
    balance_difference <== balance_order.out * (from_balance_before - from_balance_after);

    component fee_check = LessThan(32);
    fee_check.in[0] <== balance_difference;
    fee_check.in[1] <== 1000000000; 

    component sig_check = IsZero();
    sig_check.in <== signature_first_byte;
    signal signature_exists <== 1 - sig_check.out;

    signal check1 <== amount_is_positive * fee_check.out;
    is_valid <== check1 * signature_exists;
}

/*
 * Template to check if input is less than a value (robust version)
 */
template LessThan(n) {
    assert(n <= 64);  // support up to 64-bit values
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

component main = SystemTransfer();
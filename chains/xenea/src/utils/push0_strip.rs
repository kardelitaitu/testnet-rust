/// Replace PUSH0 (0x5f) with PUSH1 0x00 (0x6000) for chains that don't support Shanghai.
/// Xenea doesn't support PUSH0, so contracts compiled with Solidity 0.8.20+ need this fix.
pub fn strip_push0(bytecode: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(bytecode.len());
    let mut i = 0;
    while i < bytecode.len() {
        let byte = bytecode[i];
        if byte == 0x5f {
            // PUSH0 -> PUSH1 0x00
            result.push(0x60);
            result.push(0x00);
        } else if byte >= 0x60 && byte <= 0x7f {
            // PUSH1..PUSH32: copy opcode + its data bytes
            let n = (byte - 0x60 + 1) as usize;
            result.push(byte);
            for j in 1..n {
                if i + j < bytecode.len() {
                    result.push(bytecode[i + j]);
                }
            }
            i += n;
        } else {
            result.push(byte);
        }
        i += 1;
    }
    result
}

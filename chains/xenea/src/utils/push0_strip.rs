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
            for j in 1..=n {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_bytecode() {
        assert!(strip_push0(&[]).is_empty());
    }

    #[test]
    fn test_no_push0_passthrough() {
        let input = vec![0x00, 0x01, 0x02, 0x60, 0x01];
        assert_eq!(strip_push0(&input), input);
    }

    #[test]
    fn test_single_push0_replaced() {
        let result = strip_push0(&[0x5f]);
        assert_eq!(result, vec![0x60, 0x00]);
    }

    #[test]
    fn test_multiple_push0_replaced() {
        let result = strip_push0(&[0x5f, 0x5f, 0x5f]);
        assert_eq!(result, vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00]);
    }

    #[test]
    fn test_push1_through_push32_preserved() {
        // PUSH1 0x42 followed by PUSH2 0x1234 followed by STOP
        let input = vec![0x60, 0x42, 0x61, 0x12, 0x34, 0x00];
        let result = strip_push0(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_push0_among_other_ops() {
        // STOP PUSH0 PUSH1 0x42 PUSH0 RETURN
        let input = vec![0x00, 0x5f, 0x60, 0x42, 0x5f, 0xf3];
        let result = strip_push0(&input);
        assert_eq!(result, vec![0x00, 0x60, 0x00, 0x60, 0x42, 0x60, 0x00, 0xf3]);
    }

    #[test]
    fn test_push32_keeps_data() {
        // PUSH32 with 32 bytes of data, followed by PUSH0
        let mut input = vec![0x7f]; // PUSH32
        input.extend_from_slice(&[0xaa; 32]); // 32 data bytes
        input.push(0x5f); // PUSH0
        let result = strip_push0(&input);

        // Should preserve the PUSH32 + 32 bytes, and replace PUSH0
        let mut expected = vec![0x7f];
        expected.extend_from_slice(&[0xaa; 32]);
        expected.extend_from_slice(&[0x60, 0x00]); // PUSH1 0x00

        assert_eq!(result.len(), expected.len());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_realistic_deploy_bytecode() {
        // Minimal deployer: PUSH0 DUP1 PUSH0 DUP1 PUSH0 CODECOPY PUSH0 RETURN
        let input = vec![0x5f, 0x80, 0x5f, 0x80, 0x5f, 0x39, 0x5f, 0xf3];
        let result = strip_push0(&input);
        // Each 0x5f → 0x60 0x00
        assert_eq!(result.len(), input.len() + 4); // 4 PUSH0s → 4 extra bytes
        assert_eq!(result[0], 0x60); // first PUSH0 → PUSH1
        assert_eq!(result[1], 0x00);
        assert_eq!(result[2], 0x80); // DUP1 unchanged
    }

    #[test]
    fn test_push0_at_last_byte() {
        // PUSH0 as the very last byte — edge case for the while loop
        let input = vec![0x00, 0x5f];
        let result = strip_push0(&input);
        assert_eq!(result, vec![0x00, 0x60, 0x00]);
    }

    #[test]
    fn test_push32_data_containing_0x5f() {
        // PUSH32 where data bytes contain 0x5f — should NOT be replaced
        let mut input = vec![0x7f]; // PUSH32
        let mut data = [0xaa; 32];
        data[0] = 0x5f; // Data byte is 0x5f but NOT a PUSH0 opcode
        data[15] = 0x5f;
        data[31] = 0x5f;
        input.extend_from_slice(&data);
        let result = strip_push0(&input);
        // Should pass through unchanged — PUSH32's data is not PUSH0
        assert_eq!(result, input, "PUSH32 data containing 0x5f should not be modified");
        assert_eq!(result.len(), 33);
    }

    #[test]
    fn test_all_push_variants_adjacent() {
        // PUSH0 PUSH1 PUSH2 ... PUSH32 all adjacent — tests the loop boundaries
        let mut input = Vec::new();
        for opcode in 0x5f..=0x7f {
            input.push(opcode);
            // Append data bytes for PUSH1..PUSH32
            if opcode >= 0x60 {
                let n = (opcode - 0x60 + 1) as usize;
                for _ in 0..n {
                    input.push(0x42);
                }
            }
        }
        let result = strip_push0(&input);
        // First byte was PUSH0 (0x5f) → becomes PUSH1 0x00 (2 bytes)
        // The rest (PUSH1..PUSH32) should be preserved
        assert_eq!(result[0], 0x60);
        assert_eq!(result[1], 0x00);
        // Verify PUSH1 preserved
        assert_eq!(result[2], 0x60); // PUSH1 opcode
        assert_eq!(result[3], 0x42); // PUSH1 data
        // Total length: original - 1 (removed 0x5f) + 2 (added 0x60 0x00) = original + 1
        assert_eq!(result.len(), input.len() + 1, "All push opcodes should be handled");
    }

    #[test]
    fn test_large_bytecode_stress() {
        // Large bytecode with scattered PUSH0s — stress test
        let mut input = Vec::with_capacity(10_000);
        for i in 0..10_000 {
            if i % 7 == 0 {
                input.push(0x5f); // PUSH0 every 7th byte
            } else {
                input.push(0x00); // STOP
            }
        }
        let result = strip_push0(&input);
        // Each PUSH0 (0x5f) becomes PUSH1 0x00 (2 bytes)
        let expected_push0_count = (0..10_000).filter(|i| i % 7 == 0).count();
        assert_eq!(result.len(), input.len() + expected_push0_count);
        // Spot-check a few positions
        assert_eq!(result[0], 0x60); // First byte: was PUSH0 → PUSH1
        assert_eq!(result[1], 0x00);
    }

    #[test]
    fn test_consecutive_push32_no_interference() {
        // Two PUSH32 in a row — the second should not be affected by the first's data skip
        let mut input = vec![0x7f]; // PUSH32 #1
        input.extend_from_slice(&[0x11; 32]);
        input.push(0x7f); // PUSH32 #2
        input.extend_from_slice(&[0x22; 32]);
        let result = strip_push0(&input);
        assert_eq!(result.len(), 66, "Two PUSH32s = 2 opcodes + 64 data bytes = 66 bytes");
        assert_eq!(result, input, "Should be identical to input (no PUSH0 present)");
    }

    #[test]
    fn test_single_push0_in_long_running_code() {
        // PUSH0 in the middle of long non-PUSH code
        let mut input = vec![0x00; 100]; // 100 STOPs
        input[50] = 0x5f; // One PUSH0 at position 50
        let result = strip_push0(&input);
        assert_eq!(result.len(), 101); // 100 + 1 extra byte
        assert_eq!(result[50], 0x60); // PUSH0 → PUSH1
        assert_eq!(result[51], 0x00);
        assert_eq!(result[52], 0x00); // The next STOP
    }

    #[test]
    fn test_randomized_bytecode_invariant() {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        for _ in 0..100 {
            let len: usize = rng.gen_range(0..200);
            let mut input = Vec::with_capacity(len);
            for _ in 0..len {
                input.push(rng.gen());
            }
            let result = strip_push0(&input);

            // Invariant 1: output length >= input length (PUSH0 → PUSH1 0x00 adds 1 byte)
            assert!(result.len() >= input.len(),
                "Output shorter than input: {} < {}", result.len(), input.len());

            // Invariant 2: output should never be empty if input is non-empty
            if !input.is_empty() {
                assert!(!result.is_empty());
            }

            // Invariant 3: Input that doesn't contain 0x5f is unchanged
            if !input.contains(&0x5f) {
                assert_eq!(result, input, "Input without 0x5f should pass through unchanged");
            }
        }
    }

    #[test]
    fn test_randomized_no_push0_passthrough() {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        for _ in 0..50 {
            let len: usize = rng.gen_range(1..100);
            let mut input = Vec::with_capacity(len);
            for _ in 0..len {
                // Generate bytes in range 0x00-0x5e and 0x80-0xff (no PUSH0)
                let byte: u8 = loop {
                    let b = rng.gen();
                    if b != 0x5f {
                        break b;
                    }
                };
                input.push(byte);
            }
            let result = strip_push0(&input);
            assert_eq!(result, input, "Input without 0x5f should be unchanged");
        }
    }
}

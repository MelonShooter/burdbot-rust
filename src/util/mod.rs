use std::fmt::Display;

use aes::cipher::generic_array::GenericArray;
use aes::Aes256;
use aes::BlockDecrypt;
use aes::NewBlockCipher;

#[allow(dead_code)]
pub fn decode_aes(string: impl Display) -> String {
    let encoded_input = hex::decode(&string.to_string()[1..]).expect("Invalid string.");

    decode_aes_bytes(encoded_input)
}

#[allow(dead_code)]
pub fn decode_aes_bytes(encoded_input: Vec<u8>) -> String {
    if encoded_input.len() % 16 != 0 {
        panic!("Invalid input.");
    }

    let key = hex::decode(crate::secret::AES_KEY).expect("Bad key");

    if key.len() != 32 {
        panic!("Bad key length. Should be 256-bits.");
    }

    let mut decoded_string = String::with_capacity(encoded_input.len());
    let cipher = Aes256::new(GenericArray::from_slice(key.as_slice()));

    for start in (0..encoded_input.len()).step_by(16) {
        let mut block = *GenericArray::from_slice(&encoded_input[start..(start + 16)]);

        cipher.decrypt_block(&mut block);

        let block_vec = block.to_vec();

        let mut decoded_str = std::str::from_utf8(block_vec.as_slice()).expect("One of the decoded blocks is not UTF-8.");

        if start == 0 {
            decoded_str = decoded_str.trim_start_matches('0');
        }

        decoded_string.push_str(decoded_str);
    }

    decoded_string
}

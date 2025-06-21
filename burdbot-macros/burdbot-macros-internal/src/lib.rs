mod secret;

use std::fmt::Display;

use aes::cipher::generic_array::GenericArray;
use aes::Aes256;
use aes::BlockDecrypt;
use aes::BlockEncrypt;
use aes::NewBlockCipher;

pub fn decode_aes(string: impl Display) -> String {
    let encoded_input = hex::decode(&string.to_string()[1..]).expect("Invalid string.");

    decode_aes_bytes(encoded_input.as_slice())
}

pub fn decode_aes_bytes(encoded_input: &[u8]) -> String {
    if encoded_input.len() % 16 != 0 {
        panic!("Invalid input.");
    }

    let key = hex::decode(crate::secret::AES_KEY).expect("Bad key");

    if key.len() != 32 {
        panic!("Bad key length. Should be 256-bits.");
    }

    let mut full_block = Vec::with_capacity(encoded_input.len());
    let mut decoded_string = String::with_capacity(encoded_input.len());
    let cipher = Aes256::new(GenericArray::from_slice(key.as_slice()));

    for start in (0..encoded_input.len()).step_by(16) {
        let mut block = *GenericArray::from_slice(&encoded_input[start..(start + 16)]);

        cipher.decrypt_block(&mut block);

        let block_vec = block.to_vec();
        let mut idx = 0;

        if start == 0 {
            while idx < block_vec.len() && block_vec[idx] == b'0' {
                idx += 1;
            }
        }

        for byte in &block_vec[idx..] {
            full_block.push(*byte);
        }
    }

    let decoded_str = std::str::from_utf8(full_block.as_slice())
        .expect("One of the decoded blocks is not UTF-8.");

    decoded_string.push_str(decoded_str);

    decoded_string
}

pub fn encode_aes(str: String) -> String {
    let mut string;
    let mut str_bytes = str.as_bytes();
    let pad_count = 16 - str.len() % 16;

    if pad_count != 0 {
        string = String::with_capacity(pad_count + str_bytes.len());

        for _ in 0..pad_count {
            string.push('0');
        }

        string.push_str(str.as_str());

        str_bytes = string.as_bytes();
    }

    let key = hex::decode(crate::secret::AES_KEY).expect("Bad key.");
    let cipher = Aes256::new_from_slice(key.as_slice()).expect("Bad key.");
    let mut encoded_bytes = String::with_capacity(str_bytes.len() * 2 + 1);

    encoded_bytes.push('f');

    for start in (0..str_bytes.len()).step_by(16) {
        let mut block = *GenericArray::from_slice(&str_bytes[start..start + 16]);

        cipher.encrypt_block(&mut block);

        encoded_bytes.push_str(hex::encode(block).as_str());
    }

    encoded_bytes
}

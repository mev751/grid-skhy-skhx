use k256::ecdsa::SigningKey;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureWire {
    pub r: String,
    pub s: String,
    pub v: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRequest<T: Serialize> {
    pub action: T,
    pub nonce: u64,
    pub signature: SignatureWire,
    #[serde(rename = "vaultAddress")]
    pub vault_address: Option<String>,
}

pub struct Signer {
    signing_key: SigningKey,
    pub address: String,
}

impl Signer {
    pub fn new(secret_key_hex: &str) -> Result<Self, String> {
        let clean_hex = secret_key_hex.trim().trim_start_matches("0x");
        let bytes = hex::decode(clean_hex).map_err(|e| format!("Invalid hex private key: {}", e))?;
        if bytes.len() != 32 {
            return Err("Private key must be 32 bytes".to_string());
        }
        let signing_key = SigningKey::from_bytes(bytes.as_slice().into())
            .map_err(|e| format!("Failed to parse private key: {}", e))?;

        let verifying_key = signing_key.verifying_key();
        let encoded_point = verifying_key.to_encoded_point(false);
        let pub_bytes = encoded_point.as_bytes();
        // Skip first byte (0x04)
        let hash = Keccak256::digest(&pub_bytes[1..]);
        let address = format!("0x{}", hex::encode(&hash[12..]));

        Ok(Self {
            signing_key,
            address,
        })
    }

    pub fn get_timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Sign a Hyperliquid action with EIP-712 Phantom Agent pattern
    pub fn sign_action<T: Serialize>(
        &self,
        action: &T,
        vault_address: Option<&str>,
        nonce: u64,
        is_mainnet: bool,
    ) -> Result<SignatureWire, String> {
        // 1. Msgpack serialization
        let mut action_bytes = rmp_serde::to_vec_named(action)
            .map_err(|e| format!("Msgpack serialization error: {}", e))?;

        // 2. Append nonce as 8-byte big-endian
        action_bytes.extend_from_slice(&nonce.to_be_bytes());

        // 3. Vault address
        match vault_address {
            None => action_bytes.push(0x00),
            Some(addr) => {
                action_bytes.push(0x01);
                let clean_addr = addr.trim().trim_start_matches("0x");
                let addr_bytes = hex::decode(clean_addr)
                    .map_err(|e| format!("Invalid vault address hex: {}", e))?;
                if addr_bytes.len() != 20 {
                    return Err("Vault address must be 20 bytes".to_string());
                }
                action_bytes.extend_from_slice(&addr_bytes);
            }
        }

        // 4. Connection ID (Keccak256 of payload)
        let connection_id = Keccak256::digest(&action_bytes);

        // 5. EIP-712 Digest
        let digest = Self::calculate_eip712_digest(&connection_id, is_mainnet)?;

        // 6. Sign digest with secp256k1 recoverable
        let (signature, recovery_id) = self
            .signing_key
            .sign_prehash_recoverable(&digest)
            .map_err(|e| format!("Signing error: {}", e))?;

        let r_bytes = signature.r().to_bytes();
        let s_bytes = signature.s().to_bytes();
        let v = recovery_id.to_byte() + 27;

        Ok(SignatureWire {
            r: format!("0x{}", hex::encode(r_bytes)),
            s: format!("0x{}", hex::encode(s_bytes)),
            v,
        })
    }

    fn calculate_eip712_digest(
        connection_id: &[u8],
        is_mainnet: bool,
    ) -> Result<[u8; 32], String> {
        // Domain Separator:
        // EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)
        let domain_type_hash =
            Keccak256::digest(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
        let name_hash = Keccak256::digest(b"Exchange");
        let version_hash = Keccak256::digest(b"1");

        let mut chain_id_bytes = [0u8; 32];
        chain_id_bytes[30] = (1337 >> 8) as u8;
        chain_id_bytes[31] = (1337 & 0xff) as u8;

        let verifying_contract_bytes = [0u8; 32]; // 0x0000000000000000000000000000000000000000

        let mut domain_buf = Vec::with_capacity(32 * 5);
        domain_buf.extend_from_slice(&domain_type_hash);
        domain_buf.extend_from_slice(&name_hash);
        domain_buf.extend_from_slice(&version_hash);
        domain_buf.extend_from_slice(&chain_id_bytes);
        domain_buf.extend_from_slice(&verifying_contract_bytes);
        let domain_separator = Keccak256::digest(&domain_buf);

        // Agent Struct Hash:
        // Agent(string source,bytes32 connectionId)
        let agent_type_hash = Keccak256::digest(b"Agent(string source,bytes32 connectionId)");
        let source_str = if is_mainnet { "a" } else { "b" };
        let source_hash = Keccak256::digest(source_str.as_bytes());

        let mut message_buf = Vec::with_capacity(32 * 3);
        message_buf.extend_from_slice(&agent_type_hash);
        message_buf.extend_from_slice(&source_hash);
        message_buf.extend_from_slice(connection_id);
        let message_hash = Keccak256::digest(&message_buf);

        // Final EIP-712 Digest: \x19\x01 ++ domain_separator ++ message_hash
        let mut final_buf = Vec::with_capacity(2 + 32 + 32);
        final_buf.extend_from_slice(b"\x19\x01");
        final_buf.extend_from_slice(&domain_separator);
        final_buf.extend_from_slice(&message_hash);

        let final_digest = Keccak256::digest(&final_buf);
        let mut out = [0u8; 32];
        out.copy_from_slice(&final_digest);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signer_address_and_signing() {
        // Known test key: 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let signer = Signer::new(key).expect("Should create signer");
        assert!(signer.address.starts_with("0x"));
        assert_eq!(signer.address.len(), 42);

        #[derive(Serialize)]
        struct DummyAction {
            #[serde(rename = "type")]
            action_type: String,
        }

        let action = DummyAction {
            action_type: "test".to_string(),
        };

        let sig = signer
            .sign_action(&action, None, 1700000000000, true)
            .expect("Should sign action");

        assert!(sig.r.starts_with("0x"));
        assert!(sig.s.starts_with("0x"));
        assert!(sig.v == 27 || sig.v == 28);
    }
}


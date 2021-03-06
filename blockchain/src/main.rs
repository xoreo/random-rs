use blake3;
use ed25519_dalek::{Keypair, PublicKey, Signature};
use rand::rngs::OsRng;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use std::fs::File;
use std::io::prelude::*;
use std::time::Instant;

const HASH_SIZE: usize = 32;
type Hash = [u8; HASH_SIZE];
type Address = Hash;

#[derive(Debug)]
enum Valid {
    Valid,
    Invalid,
}

fn gen_nonce() -> f64 {
    rand::thread_rng().gen::<f64>()
}

#[derive(Serialize, Deserialize, Debug)]
struct User {
    address: Address,
    timestamp: u128,
    nonce: f64,
    public_key: PublicKey,
    uid: String,
}

impl User {
    fn new(uid: &str) -> Self {
        let mut user = Self {
            address: [0; HASH_SIZE],
            timestamp: Instant::now().elapsed().as_millis(),
            nonce: gen_nonce(),
            public_key: User::gen_keypair(uid).public,
            uid: String::from(uid),
        };
        user.hash();
        user
    }

    fn to_disk(&mut self) {
        let mut f = File::create(format!("data/{}.user", self.uid))
            .expect("Could not create user file");
        f.write_all(
            &bincode::serialize(self).expect("Could not serialize user")[..],
        )
        .expect("Could not write to user file");
    }

    fn from_uid(uid: &str) -> Self {
        let mut f = File::open(format!("data/{}.user", uid))
            .expect("Could not open user file");
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer)
            .expect("Could not read from user file");

        let user: Self = bincode::deserialize(&buffer[..])
            .expect("Could not deserialize user");

        user
    }

    fn gen_keypair(uid: &str) -> Keypair {
        let mut csprng = OsRng::new().unwrap();
        let keypair = Keypair::generate::<Sha512, _>(&mut csprng);

        let mut f = File::create(format!("secret/{}.priv", uid))
            .expect("Could not create user private key file");
        f.write_all(&keypair.to_bytes());

        keypair
    }

    fn get_keypair(uid: &str) -> Keypair {
        let mut f = File::open(format!("secret/{}.priv", uid))
            .expect("Could not open secret file");

        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer)
            .expect("Could not read from secret file");

        let keypair = Keypair::from_bytes(&buffer[..])
            .expect("Could not deserialize secret");

        keypair
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Txn {
    id: Hash,
    sender: Address,
    recipient: Address,
    amount: f64,
    timestamp: u128,
    signature: Vec<u8>,
}

trait CanSerialize {
    fn to_bytes(&self) -> Vec<u8>;
}

impl CanSerialize for Txn {
    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Could not serialize transaction")
    }
}

impl Txn {
    fn new(sender: &User, recipient: &User, amount: f64) -> Self {
        let mut txn = Self {
            id: [0; HASH_SIZE],
            sender: sender.address,
            recipient: recipient.address,
            amount,
            timestamp: Instant::now().elapsed().as_millis(),
            signature: Vec::new(),
        };
        txn.hash();
        txn
    }

    // Needs the public key only
    fn verify(&self, key: PublicKey) -> Valid {
        let signature = Signature::from_bytes(&self.signature)
            .expect("Invalid signature");
        let no_sig = Self {
            signature: Vec::new(),
            ..*self
        };
        let no_sig: &[u8] = &no_sig.to_bytes()[..];

        match key.verify::<Sha512>(no_sig, &signature) {
            Ok(_) => return Valid::Valid,
            Err(_) => return Valid::Invalid,
        }
    }

    // Needs the private key
    fn sign(&mut self, key: &Keypair) {
        let self_bytes = &self.to_bytes()[..]; // Serialize self
        let signature = key.sign::<Sha512>(self_bytes); // Calc the signature
        self.signature = signature.to_bytes().to_vec(); // Set the signature
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Txns {
    txns: Vec<Txn>,
    merkle_root: Hash,
}

impl Txns {
    fn new() -> Self {
        Self {
            txns: Vec::new(),
            merkle_root: [0; HASH_SIZE],
        }
    }

    fn add(&mut self, txn: Txn) {
        self.txns.push(txn);
    }

    fn verify(&self) -> Valid {
        Valid::Valid
    } // Just verify all of them

    fn calc_merkle_root_r(leaves: &mut Vec<Hash>) -> Hash {
        if leaves.len() == 1 {
            return *leaves
                .first()
                .expect("Could not get last transaction");
        }

        if leaves.len() % 2 != 0 {
            leaves.push(
                *leaves.last().expect("Could not get last transaction"),
            );
        }

        let mut branches: Vec<Hash> = Vec::new();

        for i in (0..leaves.len() - 1).step_by(2) {
            let mut concat: [u8; HASH_SIZE * 2] = [0; HASH_SIZE * 2];
            for j in 0..leaves[i].len() {
                concat[j] = leaves[i][j];
            }

            for j in 0..leaves[i + 1].len() {
                concat[j + HASH_SIZE] = leaves[i][j];
            }
            branches.push(*blake3::hash(&concat).as_bytes());
        }
        Txns::calc_merkle_root_r(&mut branches)
    }

    fn calc_merkle_root(&mut self) {
        let mut merkle_leaves: Vec<Hash> =
            (&self.txns).into_iter().map(|txn| txn.id).collect();
        self.merkle_root = Txns::calc_merkle_root_r(&mut merkle_leaves);
    }
}

trait Hashable {
    fn hash(&mut self);
}

impl Hashable for Block {
    fn hash(&mut self) {
        let bytes =
            &bincode::serialize(self).expect("Could not serialize block");
        self.hash = *blake3::hash(bytes).as_bytes();
    }
}

impl Hashable for Txn {
    fn hash(&mut self) {
        let bytes = &self.to_bytes();
        self.id = *blake3::hash(bytes).as_bytes();
    }
}

impl Hashable for User {
    fn hash(&mut self) {
        let bytes =
            &bincode::serialize(self).expect("Could not serialize user");
        self.address = *blake3::hash(bytes).as_bytes();
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Block {
    hash: Hash,
    prev_hash: Hash,
    txns: Txns,
    index: u32,
    timestamp: u128,
    nonce: f64,
}

impl Block {
    fn new(prev_hash: Hash, txns: Txns, index: u32) -> Self {
        let mut block = Self {
            hash: [0; HASH_SIZE],
            prev_hash,
            txns,
            index,
            nonce: gen_nonce(),
            timestamp: Instant::now().elapsed().as_millis(),
        };
        block.hash();
        block
    }
}

#[derive(Serialize, Deserialize)]
struct Blockchain {
    blocks: Vec<Block>,
    timestamp: u128,
}

impl Blockchain {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            timestamp: Instant::now().elapsed().as_millis(),
        }
    }

    fn add_block(&mut self, block: Block) {
        self.blocks.push(block);
    }

    fn verify(&self) -> Valid {
        Valid::Valid
    }
}

fn main() {
    // Make some users
    let user1 = User::from_uid("new_user");
    let user1_privkey = User::get_keypair("new_user");
    let user2 = User::new("user2");

    // Make some txns
    let mut txns1 = Txns::new();
    for amount in vec![10.0, 11.0, 12.0] {
        let mut txn = Txn::new(&user1, &user2, amount);
        txn.sign(&user1_privkey);
        txns1.add(txn);
    }
    txns1.calc_merkle_root(); // Calc the merkle root hash
    assert!(match txns1.verify() {
        Valid::Valid => true,
        Valid::Invalid => false,
    }); // Verify the txns

    // Make some more txns
    let mut txns2 = Txns::new();
    for amount in vec![20.0, 21.0, 22.0] {
        txns2.add(Txn::new(&user1, &user2, amount));
    }
    txns2.calc_merkle_root(); // Calc the merkle root hash
    assert!(match txns2.verify() {
        Valid::Valid => true,
        Valid::Invalid => false,
    }); // Verify the txns

    // Make some blocks
    let block1 = Block::new([0; HASH_SIZE], txns1, 0);
    println!("Made a new block! {:?}", block1);

    let block2 = Block::new(block1.hash, txns2, 1);
    println!("Made a new block! {:?}", block2);

    let mut blockchain = Blockchain::new();
    blockchain.add_block(block1);
    blockchain.add_block(block2);

    /* ----- VALIDATION ----- */
    let t_txn = &blockchain.blocks[0].txns.txns[0];
    println!(
        "txn 0 in block 0 is {}",
        match t_txn.verify(user1.public_key) {
            Valid::Valid => "valid!",
            Valid::Invalid => "invalid!",
        }
    );
}

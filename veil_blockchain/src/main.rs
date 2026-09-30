use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Server, Response, Header};
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Signature, Verifier};
use rand::rngs::OsRng;
use std::collections::HashMap;

// --- STRUCTURES DE LA BLOCKCHAIN ---

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Transaction {
    sender: String,
    receiver: String,
    amount: u64,
    signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Block {
    index: u64,
    timestamp: u64,
    transactions: Vec<Transaction>,
    prev_hash: String,
    hash: String,
    nonce: u64,
}

struct Blockchain {
    chain: Vec<Block>,
    difficulty: usize,
    mempool: Vec<Transaction>,
    file_path: String,
}

impl Blockchain {
    fn new(difficulty: usize, file_path: &str) -> Self {
        let mut bc = Blockchain {
            chain: Vec::new(),
            difficulty,
            mempool: Vec::new(),
            file_path: file_path.to_string(),
        };

        if let Ok(mut file) = File::open(&bc.file_path) {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(decoded_chain) = serde_json::from_str(&contents) {
                    bc.chain = decoded_chain;
                    println!("Blockchain chargée depuis le disque ({} blocs).", bc.chain.len());
                    return bc;
                }
            }
        }

        bc.create_genesis_block();
        bc
    }

    fn save_to_disk(&self) {
        if let Ok(serialized) = serde_json::to_string(&self.chain) {
            let _ = fs::write(&self.file_path, serialized);
        }
    }

    fn create_genesis_block(&mut self) {
        let genesis_block = Block {
            index: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            transactions: vec![],
            prev_hash: "0".to_string(),
            hash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            nonce: 0,
        };
        self.chain.push(genesis_block);
        self.save_to_disk();
    }

    fn latest_block(&self) -> &Block {
        self.chain.last().unwrap()
    }

    fn get_all_balances(&self) -> HashMap<String, i64> {
        let mut balances: HashMap<String, i64> = HashMap::new();

        for block in &self.chain {
            for tx in &block.transactions {
                if tx.sender != "Reseau" {
                    *balances.entry(tx.sender.clone()).or_insert(100) -= tx.amount as i64;
                }
                *balances.entry(tx.receiver.clone()).or_insert(100) += tx.amount as i64;
            }
        }

        for tx in &self.mempool {
            if tx.sender != "Reseau" {
                *balances.entry(tx.sender.clone()).or_insert(100) -= tx.amount as i64;
            }
            *balances.entry(tx.receiver.clone()).or_insert(100) += tx.amount as i64;
        }

        balances
    }

    fn verify_transaction(&self, tx: &Transaction) -> bool {
        if tx.sender == "Reseau" {
            return true;
        }

        let pub_bytes = match hex::decode(&tx.sender) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig_bytes = match hex::decode(&tx.signature) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let verifying_key = match VerifyingKey::from_bytes(&pub_bytes.try_into().unwrap_or([0; 32])) {
            Ok(k) => k,
            Err(_) => return false,
        };

        let signature = match Signature::try_from(sig_bytes.as_slice()) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let message = format!("{}:{}:{}", tx.sender, tx.receiver, tx.amount);
        verifying_key.verify(message.as_bytes(), &signature).is_ok()
    }

    fn add_transaction(&mut self, tx: Transaction) -> Result<(), &'static str> {
        if !self.verify_transaction(&tx) {
            return Err("Signature cryptographique invalide !");
        }

        if tx.sender != "Reseau" {
            let balances = self.get_all_balances();
            let balance = *balances.get(&tx.sender).unwrap_or(&100);
            if balance < tx.amount as i64 {
                return Err("Solde insuffisant pour effectuer ce transfert.");
            }
        }

        self.mempool.push(tx);
        Ok(())
    }

    fn mine_pending_transactions(&mut self) {
        let prev_block = self.latest_block();
        let index = prev_block.index + 1;
        let prev_hash = prev_block.hash.clone();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut transactions = self.mempool.clone();
        transactions.push(Transaction {
            sender: "Reseau".to_string(),
            receiver: "Miner_Node".to_string(),
            amount: 10,
            signature: "REWARD".to_string(),
        });
        self.mempool.clear();

        let mut nonce = 0u64;
        let target = "0".repeat(self.difficulty);

        loop {
            let block_data = format!("{}{}{}{}{}", index, timestamp, transactions.len(), prev_hash, nonce);
            let mut hasher = Sha256::new();
            hasher.update(block_data.as_bytes());
            let result = hasher.finalize();
            let hash = format!("{:x}", result);

            if hash.starts_with(&target) {
                let new_block = Block {
                    index,
                    timestamp,
                    transactions,
                    prev_hash,
                    hash,
                    nonce,
                };
                println!("Bloc #{} miné avec succès !", new_block.index);
                self.chain.push(new_block);
                self.save_to_disk();
                break;
            }
            nonce += 1;
        }
    }
}

// --- INTERFACE WEB HTML ---

fn render_html(blockchain: &Blockchain, message: &str, new_wallet: Option<(String, String)>) -> String {
    let balances = blockchain.get_all_balances();
    let mut accounts_html = String::new();
    if balances.is_empty() {
        accounts_html.push_str("<p>Aucun compte actif pour le moment.</p>");
    } else {
        for (addr, bal) in &balances {
            accounts_html.push_str(&format!(
                r#"<div style="display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #33334d;">
                    <code style="color: #00d2ff; font-size: 13px;">{}</code>
                    <b style="color: #00ffcc;">{} UMB</b>
                </div>"#,
                addr, bal
            ));
        }
    }

    let mut blocks_html = String::new();
    for block in blockchain.chain.iter().rev() {
        let mut txs_html = String::new();
        for tx in &block.transactions {
            let sender_short = if tx.sender == "Reseau" { "Reseau".to_string() } else { format!("{}...", &tx.sender[..8]) };
            let receiver_short = format!("{}...", &tx.receiver[..8]);
            txs_html.push_str(&format!(r#"<li><code style="color:#00d2ff;">{}</code> -> <code style="color:#00ffcc;">{}</code> : <b>{} UMB</b></li>"#, sender_short, receiver_short, tx.amount));
        }

        blocks_html.push_str(&format!(
            r#"
            <div style="background: #1e1e2f; border: 1px solid #33334d; border-radius: 8px; padding: 20px; margin-bottom: 15px;">
                <h3 style="color: #00d2ff; margin-top: 0;">Bloc #{}</h3>
                <p><strong>Timestamp :</strong> {} | <strong>Nonce :</strong> {}</p>
                <p><strong>Hash :</strong> <span style="font-family: monospace; color: #00ffcc;">{}</span></p>
                <p><strong>Transactions :</strong></p>
                <ul style="margin: 0; padding-left: 20px;">{}</ul>
            </div>
            "#,
            block.index, block.timestamp, block.nonce, block.hash, txs_html
        ));
    }

    let wallet_box = match new_wallet {
        Some((pub_k, priv_k)) => format!(
            r#"<div style="background: #233a3a; border: 1px solid #00ffcc; padding: 15px; border-radius: 8px; margin-bottom: 20px;">
                <h3 style="color: #00ffcc; margin-top: 0;">🔑 Votre Nouveau Portefeuille (sauvegardez-le bien !)</h3>
                <p><b>Adresse (Clé Publique) :</b><br><code style="word-break: break-all; color: #00d2ff;">{}</code></p>
                <p><b>Clé Privée (À garder secrète !) :</b><br><code style="word-break: break-all; color: #ff5555;">{}</code></p>
            </div>"#,
            pub_k, priv_k
        ),
        None => "".to_string(),
    };

    format!(
        r#"<!DOCTYPE html>
        <html lang="fr">
        <head>
            <meta charset="UTF-8">
            <title>Explorateur Umbra (UMB)</title>
            <style>
                body {{ background-color: #12121c; color: #e0e0e6; font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 0; padding: 20px; }}
                .container {{ max-width: 900px; margin: 0 auto; }}
                h1, h2 {{ color: #ffffff; border-bottom: 2px solid #33334d; padding-bottom: 10px; }}
                .card {{ background: #1e1e2f; border: 1px solid #33334d; padding: 20px; border-radius: 8px; margin-bottom: 20px; }}
                input, button {{ background: #2a2a40; color: #fff; border: 1px solid #444; padding: 10px; border-radius: 5px; margin: 5px 0; width: 100%; box-sizing: border-box; }}
                button {{ background: #00d2ff; color: #12121c; font-weight: bold; cursor: pointer; }}
                button:hover {{ background: #00a8cc; }}
                .btn-wallet {{ background: #00ffcc; }}
                .btn-wallet:hover {{ background: #00cca3; }}
                .msg {{ background: #233; color: #0df; padding: 10px; border-radius: 5px; margin-bottom: 15px; }}
            </style>
        </head>
        <body>
            <div class="container">
                <h1>🌑 Umbra (UMB) - Explorateur Layer 1</h1>
                {}
                {}
                <div class="card">
                    <h2>1. Créer un Portefeuille</h2>
                    <p style="font-size: 13px; color: #aaa;">Générez une nouvelle identité cryptographique. Copiez bien votre clé privée pour pouvoir dépenser vos fonds par la suite !</p>
                    <form action="/create_wallet" method="POST">
                        <button type="submit" class="btn-wallet">Générer une paire de clés</button>
                    </form>
                </div>

                <div class="card">
                    <h2>2. Espace Comptes & Soldes</h2>
                    {}
                </div>

                <div class="card">
                    <h2>3. Envoyer des UMB</h2>
                    <form action="/send" method="POST">
                        <label>Clé Privée de l'expéditeur :</label>
                        <input type="text" name="private_key" placeholder="Collez votre clé privée ici..." required>
                        <label>Adresse du destinataire (Clé publique) :</label>
                        <input type="text" name="receiver" placeholder="Adresse du destinataire..." required>
                        <label>Montant (UMB) :</label>
                        <input type="number" name="amount" placeholder="ex: 10" required>
                        <button type="submit">Signer et Transférer</button>
                    </form>
                </div>
                <h2>Registre des Blocs ({})</h2>
                {}
            </div>
        </body>
        </html>"#,
        if message.is_empty() { "".to_string() } else { format!(r#"<div class="msg">{}</div>"#, message) },
        wallet_box,
        accounts_html,
        blockchain.chain.len(),
        blocks_html
    )
}

// --- FONCTION PRINCIPALE ---

fn main() {
    let blockchain = Arc::new(Mutex::new(Blockchain::new(3, "blockchain.json")));

    let bc_clone = Arc::clone(&blockchain);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(20));
            let mut bc = bc_clone.lock().unwrap();
            bc.mine_pending_transactions();
        }
    });

    let port_str = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let port: u16 = port_str.parse().unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);

    let server = Server::http(&addr).unwrap();
    println!("🚀 Nœud Umbra en ligne sur le port {}", port);

    for mut request in server.incoming_requests() {
        let url = request.url().to_string();
        let mut message = String::new();
        let mut new_wallet = None;

        if url.starts_with("/create_wallet") {
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);
            let verifying_key: VerifyingKey = (&signing_key).into();

            let priv_hex = hex::encode(signing_key.to_bytes());
            let pub_hex = hex::encode(verifying_key.to_bytes());

            new_wallet = Some((pub_hex, priv_hex));
            message = "Portefeuille généré avec succès ! Conservez bien votre clé privée ci-dessous.".to_string();
        } else if url.starts_with("/send") {
            let mut content = String::new();
            if request.as_reader().read_to_string(&mut content).is_ok() {
                let mut priv_key_hex = String::new();
                let mut receiver = String::new();
                let mut amount: u64 = 0;

                for pair in content.split('&') {
                    let kv: Vec<&str> = pair.split('=').collect();
                    if kv.len() == 2 {
                        let val = urlencoding::decode(kv[1]).unwrap_or_default().into_owned();
                        match kv[0] {
                            "private_key" => priv_key_hex = val,
                            "receiver" => receiver = val,
                            "amount" => amount = val.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }

                if let Ok(priv_bytes) = hex::decode(&priv_key_hex.trim()) {
                    if let Ok(array) = priv_bytes.try_into() {
                        let signing_key = SigningKey::from_bytes(&array);
                        let verifying_key: VerifyingKey = (&signing_key).into();
                        let sender = hex::encode(verifying_key.to_bytes());

                        let message_to_sign = format!("{}:{}:{}", sender, receiver, amount);
                        let signature: Signature = signing_key.sign(message_to_sign.as_bytes());
                        let signature_hex = hex::encode(signature.to_bytes());

                        let tx = Transaction {
                            sender,
                            receiver,
                            amount,
                            signature: signature_hex,
                        };

                        let mut bc = blockchain.lock().unwrap();
                        match bc.add_transaction(tx) {
                            Ok(()) => message = "Transaction signée et ajoutée à la mempool avec succès !".to_string(),
                            Err(e) => message = format!("Erreur : {}", e),
                        }
                    } else {
                        message = "Erreur : Clé privée invalide.".to_string();
                    }
                } else {
                    message = "Erreur : Format de clé privée incorrect.".to_string();
                }
            }
        }

        let bc = blockchain.lock().unwrap();
        let html_content = render_html(&bc, &message, new_wallet);
        drop(bc);

        let response = Response::from_string(html_content)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
        let _ = request.respond(response);
    }
}
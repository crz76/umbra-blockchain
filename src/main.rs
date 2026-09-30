use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use tiny_http::{Server, Response};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub signature: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    pub index: u64,
    pub timestamp: u128,
    pub transactions: Vec<Transaction>,
    pub prev_hash: String,
    pub hash: String,
    pub nonce: u64,
}

pub struct Blockchain {
    pub chain: Vec<Block>,
    pub pending_transactions: Vec<Transaction>,
}

impl Blockchain {
    pub fn new() -> Self {
        let mut bc = Self {
            chain: Vec::new(),
            pending_transactions: Vec::new(),
        };
        bc.create_genesis_block();
        bc
    }

    fn create_genesis_block(&mut self) {
        let genesis_block = Block {
            index: 0,
            timestamp: 0,
            transactions: Vec::new(),
            prev_hash: "0".to_string(),
            hash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            nonce: 0,
        };
        self.chain.push(genesis_block);
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        let mut balance = 200; // Solde initial de départ pour tester
        for block in &self.chain {
            for tx in &block.transactions {
                if tx.sender == address {
                    balance -= tx.amount;
                }
                if tx.recipient == address {
                    balance += tx.amount;
                }
            }
        }
        balance
    }

    pub fn mine_pending_transactions(&mut self, miner_address: &str) {
        let prev_block = self.chain.last().unwrap();
        let mut txs = self.pending_transactions.clone();
        
        // Récompense de minage pour le mineur
        txs.push(Transaction {
            sender: "SYSTEM_REWARD".to_string(),
            recipient: miner_address.to_string(),
            amount: 50,
            signature: "REWARD".to_string(),
        });

        let new_block = Block {
            index: prev_block.index + 1,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            transactions: txs,
            prev_hash: prev_block.hash.clone(),
            hash: format!("{:x}", Sha256::digest(b"block")),
            nonce: 0,
        };
        self.chain.push(new_block);
        self.pending_transactions.clear();
    }
}

fn main() {
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));
    let server = Server::http("0.0.0.0:8080").unwrap();
    println!("🚀 Nœud Umbra en ligne sur le port 8080 !");

    for request in server.incoming_requests() {
        let url = request.url().to_string();

        if url.starts_with("/api/create_wallet") {
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);
            let verifying_key: VerifyingKey = (&signing_key).into();
            let priv_hex = hex::encode(signing_key.to_bytes());
            let pub_hex = hex::encode(verifying_key.to_bytes());

            let json = format!(r#"{{"address": "{}", "private_key": "{}"}}"#, pub_hex, priv_hex);
            let response = Response::from_string(json).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            let _ = request.respond(response);
            continue;
        }
        
        if url.starts_with("/api/balance") {
            let parts: Vec<&str> = url.split("?address=").collect();
            let balance = if parts.len() > 1 {
                let bc = blockchain.lock().unwrap();
                bc.get_balance(parts[1])
            } else {
                0
            };
            let json = format!(r#"{{"balance": {}}}"#, balance);
            let response = Response::from_string(json).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            let _ = request.respond(response);
            continue;
        }

        if url.starts_with("/api/mine") {
            let parts: Vec<&str> = url.split("?miner=").collect();
            let miner = if parts.len() > 1 { parts[1] } else { "miner_default" };
            let mut bc = blockchain.lock().unwrap();
            bc.mine_pending_transactions(miner);
            let response = Response::from_string(r#"{"status": "success", "message": "Bloc miné avec succès ! 50 UMB de récompense versés."}"#)
                .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            let _ = request.respond(response);
            continue;
        }

        if url.starts_with("/api/send") {
            let parts: Vec<&str> = url.split('?').collect();
            if parts.len() > 1 {
                let mut priv_key_hex = String::new();
                let mut recipient = String::new();
                let mut amount: u64 = 0;

                for kv in parts[1].split('&') {
                    let kv_arr: Vec<&str> = kv.split('=').collect();
                    if kv_arr.len() == 2 {
                        let val = urlencoding::decode(kv_arr[1]).unwrap_or_default().into_owned();
                        if kv_arr[0] == "priv" { priv_key_hex = val; }
                        if kv_arr[0] == "to" { recipient = val; }
                        if kv_arr[0] == "amount" { amount = val.parse().unwrap_or(0); }
                    }
                }

                if let Ok(priv_bytes) = hex::decode(priv_key_hex.trim()) {
                    if priv_bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&priv_bytes);
                        let signing_key = SigningKey::from_bytes(&arr);
                        let verifying_key: VerifyingKey = (&signing_key).into();
                        let sender = hex::encode(verifying_key.to_bytes());

                        let msg = format!("{}{}{}", sender, recipient, amount);
                        let signature = signing_key.sign(msg.as_bytes());
                        let signature_hex = hex::encode(signature.to_bytes());

                        let tx = Transaction {
                            sender,
                            recipient,
                            amount,
                            signature: signature_hex,
                        };

                        let mut bc = blockchain.lock().unwrap();
                        bc.pending_transactions.push(tx);

                        let resp = Response::from_string(r#"{"status": "success", "message": "Transaction ajoutée à la file d'attente !"}"#);
                        let _ = request.respond(resp);
                        continue;
                    }
                }
            }
            let resp = Response::from_string(r#"{"status": "error", "message": "Erreur de signature ou clés invalides"}"#);
            let _ = request.respond(resp);
            continue;
        }

        let html = r#"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="UTF-8">
    <title>Umbra - Layer 1 Blockchain</title>
    <style>
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #0b0f19; color: #f8fafc; margin: 0; padding: 20px; }
        .container { max-width: 900px; margin: auto; }
        h1 { color: #38bdf8; text-align: center; margin-bottom: 30px; }
        .card { background: #1e293b; border: 1px solid #334155; padding: 20px; border-radius: 10px; margin-bottom: 20px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); }
        input, select, button { width: 100%; padding: 12px; margin-top: 8px; margin-bottom: 15px; background: #0f172a; border: 1px solid #475569; color: white; border-radius: 6px; box-sizing: border-box; }
        button { background: #0284c7; font-weight: bold; cursor: pointer; transition: background 0.2s; }
        button:hover { background: #0369a1; }
        .btn-miner { background: #10b981; }
        .btn-miner:hover { background: #059669; }
        .account-box { background: #0f172a; border: 1px solid #3b82f6; padding: 15px; border-radius: 8px; margin-top: 15px; }
        .miner-box { border-color: #10b981; background: #064e3b22; }
        .mono { font-family: monospace; font-size: 0.85em; color: #38bdf8; word-break: break-all; }
        .badge { display: inline-block; padding: 3px 8px; border-radius: 4px; font-size: 0.75em; font-weight: bold; }
        .badge-user { background: #0284c7; color: white; }
        .badge-miner { background: #10b981; color: white; }
        .balance { font-size: 1.2em; color: #34d399; font-weight: bold; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🌑 Umbra Blockchain - Dashboard</h1>
        
        <div class="card">
            <h3>📂 Gestion des Comptes & Espaces Privés</h3>
            <p style="color: #94a3b8; font-size: 0.9em;">Créez des comptes utilisateurs ou des nœuds de minage distincts. Leurs clés et adresses restent sauvegardées en permanence.</p>
            <div style="display: flex; gap: 10px;">
                <button onclick="createWallet('user')">👤 Créer un Compte Utilisateur</button>
                <button onclick="createWallet('miner')" class="btn-miner">⛏️ Créer un Compte Mineur</button>
            </div>
            <div id="accountsList"></div>
        </div>

        <div class="card">
            <h3>💸 Effectuer un Transfert UMB</h3>
            <label>Clé Privée de l'expéditeur :</label>
            <input type="text" id="sendPriv" placeholder="Sélectionnez un compte ci-dessus ou collez la clé">
            <label>Adresse du destinataire :</label>
            <input type="text" id="sendTo" placeholder="Adresse publique du destinataire">
            <label>Montant :</label>
            <input type="number" id="sendAmount" value="25">
            <button onclick="sendTx()">🚀 Envoyer les UMB</button>
            <div id="statusMsg" class="mono" style="margin-top: 10px; color: #38bdf8;"></div>
        </div>
    </div>

<script>
    function getWallets() {
        return JSON.parse(localStorage.getItem('umbra_wallets_v2') || '[]');
    }

    async function createWallet(type) {
        let res = await fetch('/api/create_wallet');
        let data = await res.json();
        
        let wallets = getWallets();
        wallets.push({
            name: type === 'miner' ? `Nœud Mineur #${wallets.filter(w=>w.type==='miner').length + 1}` : `Compte #${wallets.filter(w=>w.type==='user').length + 1}`,
            type: type,
            address: data.address,
            privKey: data.private_key
        });
        localStorage.setItem('umbra_wallets_v2', JSON.stringify(wallets));
        loadWalletsUI();
    }

    async function loadWalletsUI() {
        let wallets = getWallets();
        let html = '';
        
        for (let i = 0; i < wallets.length; i++) {
            let w = wallets[i];
            let res = await fetch(`/api/balance?address=${w.address}`);
            let data = await res.json();
            
            let badge = w.type === 'miner' ? '<span class="badge badge-miner">MINEUR</span>' : '<span class="badge badge-user">UTILISATEUR</span>';
            let boxClass = w.type === 'miner' ? 'account-box miner-box' : 'account-box';
            
            html += `<div class="${boxClass}">
                <div style="display: flex; justify-content: space-between; align-items: center;">
                    <b>${w.name}</b> ${badge}
                </div>
                <p style="margin: 8px 0;">Solde : <span class="balance">${data.balance} UMB</span></p>
                <p style="margin: 4px 0;">Adresse : <br><span class="mono">${w.address}</span></p>
                <p style="margin: 4px 0;">Clé Privée : <br><span class="mono" style="color: #f43f5e;">${w.privKey}</span></p>
                
                <div style="display: flex; gap: 10px; margin-top: 10px;">
                    <button style="margin:0; background: #334155;" onclick="selectForSend('${w.privKey}')">Utiliser pour envoyer</button>
                    ${w.type === 'miner' ? `<button style="margin:0;" class="btn-miner" onclick="mineBlock('${w.address}')">⛏️ Miner un bloc (Gagner 50 UMB)</button>` : ''}
                </div>
            </div>`;
        }
        document.getElementById('accountsList').innerHTML = html || '<p style="color: #64748b; text-align: center; margin-top: 20px;">Aucun compte créé pour le moment.</p>';
    }

    function selectForSend(priv) {
        document.getElementById('sendPriv').value = priv;
        alert("Clé privée injectée dans le formulaire d'envoi !");
    }

    async function sendTx() {
        let priv = document.getElementById('sendPriv').value;
        let to = document.getElementById('sendTo').value;
        let amount = document.getElementById('sendAmount').value;
        
        let res = await fetch(`/api/send?priv=${encodeURIComponent(priv)}&to=${encodeURIComponent(to)}&amount=${amount}`);
        let data = await res.json();
        document.getElementById('statusMsg').innerText = data.message;
        loadWalletsUI();
    }

    async function mineBlock(minerAddress) {
        let res = await fetch(`/api/mine?miner=${minerAddress}`);
        let data = await res.json();
        document.getElementById('statusMsg').innerText = data.message;
        loadWalletsUI();
    }

    loadWalletsUI();
</script>
</body>
</html>
"#;

        let response = Response::from_string(html)
            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
        let _ = request.respond(response);
    }
}
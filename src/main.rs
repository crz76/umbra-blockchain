use ed25519_dalek::{SigningKey, VerifyingKey, Signer};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
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
        
        // Récompense de minage automatique pour le serveur
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

    // ⛏️ Minage automatique en arrière-plan toutes les 30 secondes par le serveur
    let bc_clone = Arc::clone(&blockchain);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            let mut bc = bc_clone.lock().unwrap();
            let server_miner = "umbra_system_autofaucet_address";
            bc.mine_pending_transactions(server_miner);
            println!("🔄 [Automatique] Un nouveau bloc a été miné par le serveur !");
        }
    });

    let server = Server::http("0.0.0.0:8080").unwrap();
    println!("🚀 Nœud Umbra en ligne sur le port 8080 ! Minage auto actif.");

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
                        if kv_arr[0] == "priv" { priv_key_hex = val.clone(); }
                        if kv_arr[0] == "to" { recipient = val.clone(); }
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

                        let resp = Response::from_string(r#"{"status": "success", "message": "Transaction ajoutée et en attente de minage automatique !"}"#);
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
        input, button { width: 100%; padding: 12px; margin-top: 8px; margin-bottom: 15px; background: #0f172a; border: 1px solid #475569; color: white; border-radius: 6px; box-sizing: border-box; }
        button { background: #0284c7; font-weight: bold; cursor: pointer; transition: background 0.2s; }
        button:hover { background: #0369a1; }
        .account-box { background: #0f172a; border: 1px solid #3b82f6; padding: 20px; border-radius: 8px; margin-top: 20px; box-shadow: inset 0 2px 4px rgba(0,0,0,0.5); }
        .mono { font-family: monospace; font-size: 0.85em; color: #38bdf8; word-break: break-all; }
        .badge { display: inline-block; padding: 3px 8px; border-radius: 4px; font-size: 0.75em; font-weight: bold; background: #0284c7; color: white; }
        .balance { font-size: 1.3em; color: #34d399; font-weight: bold; }
        .transfer-section { margin-top: 15px; border-top: 1px dashed #334155; padding-top: 15px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🌑 Umbra Blockchain - Dashboard</h1>
        
        <div class="card">
            <h3>📂 Gestion des Comptes Actifs</h3>
            <p style="color: #94a3b8; font-size: 0.9em;">Créez ou gérez votre compte utilisateur. Le minage s'effectue automatiquement par le serveur en arrière-plan. Chaque compte affiche son solde en temps réel et intègre son propre module de transfert direct.</p>
            <button onclick="createWallet()">👤 Créer un Nouveau Compte</button>
        </div>

        <div class="card">
            <h3>💼 Vos Comptes Actifs & Transferts</h3>
            <div id="accountsList"><p style="color: #64748b; text-align: center; margin-top: 20px;">Aucun compte créé pour le moment.</p></div>
            <div id="statusMsg" class="mono" style="margin-top: 15px; text-align: center; color: #38bdf8;"></div>
        </div>
    </div>

<script>
    function getWallets() {
        return JSON.parse(localStorage.getItem('umbra_wallets_v3') || '[]');
    }

    async function createWallet() {
        let res = await fetch('/api/create_wallet');
        let data = await res.json();
        
        let wallets = getWallets();
        wallets.push({
            name: `Compte Actif #${wallets.length + 1}`,
            address: data.address,
            privKey: data.private_key
        });
        localStorage.setItem('umbra_wallets_v3', JSON.stringify(wallets));
        loadWalletsUI();
    }

    async function loadWalletsUI() {
        let wallets = getWallets();
        let html = '';
        
        for (let i = 0; i < wallets.length; i++) {
            let w = wallets[i];
            let res = await fetch(`/api/balance?address=${w.address}`);
            let data = await res.json();
            
            html += `<div class="account-box">
                <div style="display: flex; justify-content: space-between; align-items: center;">
                    <b>${w.name}</b> <span class="badge">ACTIF</span>
                </div>
                <p style="margin: 8px 0;">Solde Actuel : <span class="balance">${data.balance} UMB</span></p>
                <p style="margin: 4px 0;">Adresse Publique : <br><span class="mono">${w.address}</span></p>
                <p style="margin: 4px 0;">Clé Privée : <br><span class="mono" style="color: #f43f5e;">${w.privKey}</span></p>
                
                <div class="transfer-section">
                    <h4 style="margin: 0 0 10px 0; color: #38bdf8;">💸 Envoyer des UMB depuis ce compte</h4>
                    <label style="font-size: 0.9em; color: #94a3b8;">Adresse du destinataire :</label>
                    <input type="text" id="to_${i}" placeholder="Collez l'adresse publique du destinataire">
                    <label style="font-size: 0.9em; color: #94a3b8;">Montant à envoyer :</label>
                    <input type="number" id="amount_${i}" value="25">
                    <button style="background: #10b981; margin: 0;" onclick="sendTxFrom('${w.privKey}', ${i})">🚀 Envoyer les UMB</button>
                </div>
            </div>`;
        }
        document.getElementById('accountsList').innerHTML = html || '<p style="color: #64748b; text-align: center; margin-top: 20px;">Aucun compte créé pour le moment.</p>';
    }

    async function sendTxFrom(privKey, index) {
        let to = document.getElementById(`to_${index}`).value;
        let amount = document.getElementById(`amount_${index}`).value;
        
        if (!to) {
            alert("Veuillez renseigner l'adresse du destinataire !");
            return;
        }

        let res = await fetch(`/api/send?priv=${encodeURIComponent(privKey)}&to=${encodeURIComponent(to)}&amount=${amount}`);
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
```

### Pousse les modifications sur GitHub :
```powershell
git add src/main.rs
git commit -m "Suppression de l'espace mineur manuel et ajout des comptes actifs avec formulaires de transfert intégrés"
git push -u origin main --force
```

Le code est épuré, tout tourne de manière autonome via le serveur, et chaque utilisateur gère ses envois directement depuis sa propre carte de compte actif !
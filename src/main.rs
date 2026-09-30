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
        let mut balance = 200; // Solde initial de départ
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
        
        txs.push(Transaction {
            sender: "SYSTEM_REWARD".to_string(),
            recipient: miner_address.to_string(),
            amount: 50,
            signature: "REWARD".to_string(),
        });

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let raw_data = format!("{}{}{}", prev_block.hash, timestamp, txs.len());
        let hash = format!("{:x}", Sha256::digest(raw_data.as_bytes()));

        let new_block = Block {
            index: prev_block.index + 1,
            timestamp,
            transactions: txs,
            prev_hash: prev_block.hash.clone(),
            hash,
            nonce: 42,
        };
        self.chain.push(new_block);
        self.pending_transactions.clear();
    }
}

fn main() {
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));

    // Minage automatique en arrière-plan toutes les 30 secondes
    let bc_clone = Arc::clone(&blockchain);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            let mut bc = bc_clone.lock().unwrap();
            bc.mine_pending_transactions("umbra_system_autofaucet_address");
            println!("🔄 [Automatique] Un nouveau bloc a été miné par le système !");
        }
    });

    let server = Server::http("0.0.0.0:8080").unwrap();
    println!("🚀 Nœud Umbra v2 en ligne sur le port 8080 !");

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

        if url.starts_with("/api/verify") {
            let parts: Vec<&str> = url.split("?priv=").collect();
            let mut valid = false;
            let mut address = String::new();

            if parts.len() > 1 {
                let priv_key_hex = urlencoding::decode(parts[1]).unwrap_or_default().into_owned();
                if let Ok(priv_bytes) = hex::decode(priv_key_hex.trim()) {
                    if priv_bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&priv_bytes);
                        let signing_key = SigningKey::from_bytes(&arr);
                        let verifying_key: VerifyingKey = (&signing_key).into();
                        address = hex::encode(verifying_key.to_bytes());
                        valid = true;
                    }
                }
            }

            let json = format!(r#"{{"valid": {}, "address": "{}"}}"#, valid, address);
            let response = Response::from_string(json).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            let _ = request.respond(response);
            continue;
        }

        if url.starts_with("/api/blocks") {
            let bc = blockchain.lock().unwrap();
            let json = serde_json::to_string(&bc.chain).unwrap_or_else(|_| "[]".to_string());
            let response = Response::from_string(json).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            let _ = request.respond(response);
            continue;
        }

        if url.starts_with("/api/faucet") {
            let parts: Vec<&str> = url.split("?address=").collect();
            if parts.len() > 1 {
                let recipient = urlencoding::decode(parts[1]).unwrap_or_default().into_owned();
                let tx = Transaction {
                    sender: "FAUCET_SYSTEM".to_string(),
                    recipient,
                    amount: 50,
                    signature: "FAUCET_SIGNATURE".to_string(),
                };
                let mut bc = blockchain.lock().unwrap();
                bc.pending_transactions.push(tx);
                let resp = Response::from_string(r#"{"status": "success", "message": "50 UMB ajoutés à la mempool via le Faucet !"}"#);
                let _ = request.respond(resp);
                continue;
            }
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

                        let resp = Response::from_string(r#"{"status": "success", "message": "Transaction envoyée avec succès !"}"#);
                        let _ = request.respond(resp);
                        continue;
                    }
                }
            }
            let resp = Response::from_string(r#"{"status": "error", "message": "Paramètres ou clé invalides"}"#);
            let _ = request.respond(resp);
            continue;
        }

        let html = r#"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Umbra Blockchain - Écosystème Décentralisé</title>
    <style>
        :root {
            --bg-main: #0b0f19;
            --bg-card: #131b2e;
            --border-color: #1e293b;
            --border-highlight: #334155;
            --accent: #38bdf8;
            --accent-hover: #0284c7;
            --success: #10b981;
            --success-hover: #059669;
            --danger: #ef4444;
            --text-main: #f8fafc;
            --text-muted: #94a3b8;
        }
        body { font-family: 'Inter', 'Segoe UI', Tahoma, sans-serif; background: var(--bg-main); color: var(--text-main); margin: 0; padding: 20px; }
        .container { max-width: 900px; margin: auto; }
        header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 25px; border-bottom: 1px solid var(--border-color); padding-bottom: 15px; }
        h1 { color: var(--accent); font-size: 1.5rem; margin: 0; display: flex; align-items: center; gap: 10px; }
        
        .nav-tabs { display: flex; gap: 10px; }
        .tab-btn { background: var(--bg-card); border: 1px solid var(--border-highlight); color: var(--text-muted); padding: 8px 16px; border-radius: 8px; cursor: pointer; font-weight: 600; transition: all 0.2s; }
        .tab-btn.active { background: var(--accent); color: #fff; border-color: var(--accent); }

        .card { background: var(--bg-card); border: 1px solid var(--border-color); padding: 25px; border-radius: 16px; margin-bottom: 20px; box-shadow: 0 10px 15px -3px rgba(0,0,0,0.5); }
        input, button { width: 100%; padding: 12px 15px; margin-top: 8px; margin-bottom: 15px; background: #090d16; border: 1px solid var(--border-highlight); color: white; border-radius: 8px; box-sizing: border-box; font-size: 0.95rem; }
        input:focus { outline: none; border-color: var(--accent); }
        button { background: var(--accent-hover); font-weight: bold; cursor: pointer; transition: background 0.2s; border: none; }
        button:hover { background: var(--accent); color: #000; }
        .btn-success { background: var(--success); color: white; }
        .btn-success:hover { background: var(--success-hover); color: white; }
        .btn-danger { background: var(--danger); width: auto; padding: 6px 14px; margin: 0; }
        
        .mono { font-family: 'Fira Code', monospace; font-size: 0.85em; color: var(--accent); word-break: break-all; }
        .balance-box { font-size: 2.2rem; color: #34d399; font-weight: 800; margin: 10px 0; }
        .flex-row { display: flex; justify-content: space-between; align-items: center; }
        .copy-group { display: flex; gap: 8px; align-items: center; background: #090d16; padding: 8px 12px; border-radius: 8px; border: 1px solid var(--border-highlight); margin-top: 5px; }
        .copy-btn { background: #334155; padding: 4px 8px; font-size: 0.8rem; border-radius: 4px; width: auto; margin: 0; cursor: pointer; }
        .copy-btn:hover { background: var(--accent); color: #000; }
        
        .block-item { background: #090d16; border: 1px solid var(--border-color); padding: 15px; border-radius: 10px; margin-bottom: 12px; }
        .hidden { display: none !important; }
        .badge { background: #1e293b; padding: 2px 8px; border-radius: 4px; font-size: 0.75rem; color: var(--accent); }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>🌑 Umbra Network</h1>
            <div id="navMenu" class="nav-tabs hidden">
                <button class="tab-btn active" onclick="switchTab('wallet', event)">Portefeuille</button>
                <button class="tab-btn" onclick="switchTab('explorer', event)">Explorateur de Blocs</button>
            </div>
        </header>
        
        <!-- ÉCRAN D'AUTHENTIFICATION -->
        <div id="authView" class="card">
            <h3 style="margin-top:0;">🔑 Connexion à votre Espace Umbra</h3>
            <p style="color: var(--text-muted); font-size: 0.9em;">Retrouvez votre compte sur n'importe quel appareil ou générez-en un nouveau instantanément.</p>
            
            <label style="font-size:0.9rem; color: var(--text-muted);">Clé Privée Existante :</label>
            <input type="text" id="loginPrivKey" placeholder="Collez votre clé privée (hexadécimal)...">
            <button class="btn-success" onclick="loginWallet()">Se Connecter à mon Espace</button>

            <div style="text-align: center; margin: 20px 0; color: var(--text-muted); font-weight: bold; font-size: 0.9rem;">OU</div>

            <button onclick="createAndLoginWallet()">✨ Créer un Nouveau Compte Sécurisé</button>
            <div id="authError" style="color: var(--danger); margin-top: 10px; font-size: 0.9em; text-align: center;"></div>
        </div>

        <!-- SECTION ESPACE PERSONNEL (WALLET) -->
        <div id="walletTab" class="card hidden">
            <div class="flex-row">
                <div>
                    <h3 style="margin: 0;">👤 Tableau de Bord Personnel</h3>
                    <span class="badge">Compte Actif & Sécurisé</span>
                </div>
                <button class="btn-danger" onclick="logout()">Déconnexion</button>
            </div>

            <p style="margin: 20px 0 5px 0; color: var(--text-muted); font-size: 0.9rem;">Solde Disponible :</p>
            <div class="balance-box" id="userBalance">0 UMB</div>
            <button style="width: auto; padding: 8px 16px; font-size: 0.85rem;" class="btn-success" onclick="claimFaucet()">💧 Obtenir des UMB Test (Faucet)</button>

            <p style="margin: 20px 0 5px 0; color: var(--text-muted); font-size: 0.9rem;">Votre Adresse Publique :</p>
            <div class="copy-group">
                <div class="mono" id="userAddress" style="overflow: hidden; text-overflow: ellipsis;"></div>
                <button class="copy-btn" onclick="copyText('userAddress', this)">Copier</button>
            </div>

            <p style="margin: 20px 0 5px 0; color: var(--text-muted); font-size: 0.9rem;">Votre Clé Privée (Gardez-la secrète !) :</p>
            <div class="copy-group" style="border-color: #7f1d1d;">
                <div class="mono" id="userPrivKey" style="color: #f87171; overflow: hidden; text-overflow: ellipsis;"></div>
                <button class="copy-btn" onclick="copyText('userPrivKey', this)">Copier</button>
            </div>

            <div style="margin-top: 30px; padding-top: 25px; border-top: 1px dashed var(--border-highlight);">
                <h4 style="margin: 0 0 15px 0; color: var(--accent);">💸 Transférer des UMB</h4>
                <label style="font-size: 0.85rem; color: var(--text-muted);">Adresse du Destinataire :</label>
                <input type="text" id="sendTo" placeholder="Collez l'adresse publique du destinataire...">
                
                <label style="font-size: 0.85rem; color: var(--text-muted);">Montant (UMB) :</label>
                <input type="number" id="sendAmount" value="25" min="1">
                
                <button onclick="sendTx()">🚀 Envoyer les fonds</button>
                <div id="statusMsg" class="mono" style="margin-top: 10px; text-align: center;"></div>
            </div>
        </div>

        <!-- SECTION EXPLORATEUR DE BLOCS -->
        <div id="explorerTab" class="card hidden">
            <div class="flex-row">
                <h3 style="margin: 0;">🧱 Explorateur de Blocs en Direct</h3>
                <button style="width: auto; padding: 6px 12px; font-size: 0.85rem;" onclick="loadBlocks()">🔄 Actualiser</button>
            </div>
            <p style="color: var(--text-muted); font-size: 0.9rem; margin-bottom: 20px;">Visualisez l'ensemble des blocs validés sur la blockchain Umbra.</p>
            <div id="blocksContainer">Chargement des blocs...</div>
        </div>
    </div>

<script>
    let currentPrivKey = localStorage.getItem('umbra_active_priv');

    async function init() {
        if (currentPrivKey) {
            let res = await fetch(`/api/verify?priv=${encodeURIComponent(currentPrivKey)}`);
            let data = await res.json();
            if (data.valid) {
                showDashboard(currentPrivKey, data.address);
            } else {
                logout();
            }
        }
    }

    async function createAndLoginWallet() {
        let res = await fetch('/api/create_wallet');
        let data = await res.json();
        localStorage.setItem('umbra_active_priv', data.private_key);
        currentPrivKey = data.private_key;
        showDashboard(data.private_key, data.address);
    }

    async function loginWallet() {
        let priv = document.getElementById('loginPrivKey').value.trim();
        let res = await fetch(`/api/verify?priv=${encodeURIComponent(priv)}`);
        let data = await res.json();
        
        if (data.valid) {
            localStorage.setItem('umbra_active_priv', priv);
            currentPrivKey = priv;
            showDashboard(priv, data.address);
        } else {
            document.getElementById('authError').innerText = "Clé privée invalide. Vérifiez votre saisie.";
        }
    }

    function showDashboard(priv, address) {
        document.getElementById('authView').classList.add('hidden');
        document.getElementById('walletTab').classList.remove('hidden');
        document.getElementById('navMenu').classList.remove('hidden');
        document.getElementById('userPrivKey').innerText = priv;
        document.getElementById('userAddress').innerText = address;
        updateBalance(address);
        loadBlocks();
    }

    async function updateBalance(address) {
        let res = await fetch(`/api/balance?address=${address}`);
        let data = await res.json();
        document.getElementById('userBalance').innerText = `${data.balance} UMB`;
    }

    async function claimFaucet() {
        let address = document.getElementById('userAddress').innerText;
        let res = await fetch(`/api/faucet?address=${encodeURIComponent(address)}`);
        let data = await res.json();
        alert(data.message);
        updateBalance(address);
    }

    function logout() {
        localStorage.removeItem('umbra_active_priv');
        currentPrivKey = null;
        document.getElementById('walletTab').classList.add('hidden');
        document.getElementById('explorerTab').classList.add('hidden');
        document.getElementById('navMenu').classList.add('hidden');
        document.getElementById('authView').classList.remove('hidden');
        document.getElementById('loginPrivKey').value = '';
        document.getElementById('authError').innerText = '';
    }

    function switchTab(tabName, event) {
        document.querySelectorAll('.tab-btn').forEach(btn => btn.classList.remove('active'));
        if (tabName === 'wallet') {
            document.getElementById('walletTab').classList.remove('hidden');
            document.getElementById('explorerTab').classList.add('hidden');
            if (event && event.target) event.target.classList.add('active');
        } else {
            document.getElementById('walletTab').classList.add('hidden');
            document.getElementById('explorerTab').classList.remove('hidden');
            if (event && event.target) event.target.classList.add('active');
            loadBlocks();
        }
    }

    async function loadBlocks() {
        let res = await fetch('/api/blocks');
        let blocks = await res.json();
        let container = document.getElementById('blocksContainer');
        container.innerHTML = '';

        blocks.reverse().forEach(block => {
            let dateStr = block.timestamp === 0 ? "Bloc Genesis (0)" : new Date(block.timestamp).toLocaleString();
            let html = `
                <div class="block-item">
                    <div class="flex-row">
                        <strong style="color: var(--accent);">Bloc #${block.index}</strong>
                        <span class="badge">${dateStr}</span>
                    </div>
                    <div style="margin-top: 8px; font-size: 0.85rem; color: var(--text-muted);">Hash : <span class="mono">${block.hash}</span></div>
                    <div style="margin-top: 4px; font-size: 0.85rem; color: var(--text-muted);">Transactions : ${block.transactions.length}</div>
                </div>
            `;
            container.innerHTML += html;
        });
    }

    function copyText(elementId, btn) {
        let text = document.getElementById(elementId).innerText;
        navigator.clipboard.writeText(text);
        let oldText = btn.innerText;
        btn.innerText = "Copié !";
        setTimeout(() => btn.innerText = oldText, 2000);
    }

    async function sendTx() {
        let to = document.getElementById('sendTo').value.trim();
        let amount = document.getElementById('sendAmount').value;
        let address = document.getElementById('userAddress').innerText;

        let res = await fetch(`/api/send?priv=${encodeURIComponent(currentPrivKey)}&to=${encodeURIComponent(to)}&amount=${amount}`);
        let data = await res.json();
        
        let msgEl = document.getElementById('statusMsg');
        msgEl.style.color = data.status === 'success' ? '#34d399' : '#ef4444';
        msgEl.innerText = data.message;
        
        if (data.status === 'success') {
            updateBalance(address);
            document.getElementById('sendTo').value = '';
        }
    }

    init();
</script>
</body>
</html>
"#;

        let response = Response::from_string(html)
            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
        let _ = request.respond(response);
    }
}
use anyhow::{Context, Result};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct CredentialStore {
    db_path: String,
}

impl CredentialStore {
    pub fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_string(),
        }
    }

    /// Initialize the credentials database
    fn init_db(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS credentials (
                id INTEGER PRIMARY KEY,
                username TEXT NOT NULL,
                password_encrypted BLOB NOT NULL
            )",
            [],
        )
        .context("Failed to create credentials table")?;
        Ok(())
    }

    /// Simple XOR encryption with key derived from sqlite password
    fn encrypt_password(&self, password: &str, sqlite_password: &str) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(sqlite_password.as_bytes());
        let key = hasher.finalize();

        password
            .as_bytes()
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ key[i % key.len()])
            .collect()
    }

    /// Simple XOR decryption with key derived from sqlite password
    fn decrypt_password(&self, encrypted: &[u8], sqlite_password: &str) -> Result<String> {
        let mut hasher = Sha256::new();
        hasher.update(sqlite_password.as_bytes());
        let key = hasher.finalize();

        let decrypted: Vec<u8> = encrypted
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ key[i % key.len()])
            .collect();

        String::from_utf8(decrypted).context("Failed to decrypt password")
    }

    /// Check if credentials exist in the database
    pub fn credentials_exist(&self) -> Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        self.init_db(&conn)?;

        let mut stmt = conn.prepare("SELECT COUNT(*) FROM credentials")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;

        Ok(count > 0)
    }

    /// Store credentials in the database
    pub fn store_credentials(
        &self,
        username: &str,
        password: &str,
        sqlite_password: &str,
    ) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        self.init_db(&conn)?;

        let encrypted = self.encrypt_password(password, sqlite_password);

        // Delete existing credentials
        conn.execute("DELETE FROM credentials", [])?;

        // Insert new credentials
        conn.execute(
            "INSERT INTO credentials (id, username, password_encrypted) VALUES (1, ?1, ?2)",
            (username, encrypted),
        )
        .context("Failed to store credentials")?;

        Ok(())
    }

    /// Retrieve credentials from the database
    pub fn get_credentials(&self, sqlite_password: &str) -> Result<(String, String)> {
        let conn = Connection::open(&self.db_path)?;
        self.init_db(&conn)?;

        let mut stmt = conn.prepare("SELECT username, password_encrypted FROM credentials WHERE id = 1")?;
        let (username, encrypted): (String, Vec<u8>) = stmt.query_row([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let password = self.decrypt_password(&encrypted, sqlite_password)?;

        Ok((username, password))
    }

    /// Clear all stored credentials
    pub fn clear_credentials(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        self.init_db(&conn)?;
        conn.execute("DELETE FROM credentials", [])?;
        Ok(())
    }
}

use keyring::Entry;

const SERVICE: &str = "info.soltros.cabinet.desktop";
const ACCOUNT: &str = "cabinet-session";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

pub fn save(token: &str) -> Result<(), String> {
    entry()?.set_password(token).map_err(|e| e.to_string())
}

pub fn load() -> Option<String> {
    entry().ok()?.get_password().ok()
}

pub fn clear() {
    if let Ok(entry) = entry() {
        let _ = entry.delete_credential();
    }
}

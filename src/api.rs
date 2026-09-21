use reqwest::{
    blocking::{multipart, Client, RequestBuilder, Response},
    Method, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::File,
    io,
    path::Path,
    time::Duration,
};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub role: String,
    pub quota: i64,
    pub used_space: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CabinetFile {
    pub id: String,
    #[serde(default)]
    pub owner_id: String,
    pub name: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub size: i64,
    pub hash: Option<String>,
    pub parent_id: Option<String>,
    pub thumbnail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub id: String,
    pub file_id: String,
    pub file_name: Option<String>,
    pub expires_at: Option<String>,
    pub download_limit: Option<i64>,
    pub downloads: i64,
    pub active: Value,
    pub has_password: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminStats {
    pub total_users: i64,
    pub total_files: i64,
    pub total_shares: i64,
    pub total_storage_used: i64,
    pub total_storage_quota: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUser {
    pub id: String,
    pub username: String,
    pub quota: i64,
    pub used_space: i64,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminShare {
    pub id: String,
    pub file_id: String,
    pub creator_id: String,
    pub expires_at: Option<String>,
    pub download_limit: Option<i64>,
    pub downloads: i64,
    pub active: Value,
    pub created_at: String,
    pub has_password: Value,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
    pub creator_name: Option<String>,
}

#[derive(Clone)]
pub struct CabinetClient {
    base_url: String,
    token: String,
    http: Client,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Deserialize)]
struct MeResponse {
    user: User,
}

#[derive(Deserialize)]
struct FilesResponse {
    files: Vec<CabinetFile>,
}

#[derive(Deserialize)]
struct FoldersResponse {
    folders: Vec<Folder>,
}

#[derive(Deserialize)]
struct FolderResponse {
    folder: Folder,
}

#[derive(Deserialize)]
struct FileResponse {
    file: CabinetFile,
}

#[derive(Deserialize)]
struct SharesResponse {
    shares: Vec<Share>,
}

#[derive(Deserialize)]
struct AdminUsersResponse {
    users: Vec<AdminUser>,
}

#[derive(Deserialize)]
struct AdminSharesResponse {
    shares: Vec<AdminShare>,
}

#[derive(Deserialize)]
struct ShareResponse {
    link: String,
}

impl CabinetClient {
    pub fn normalize_server_url(input: &str) -> Result<String, String> {
        let candidate = input.trim().trim_end_matches('/');
        let parsed = Url::parse(candidate).map_err(|_| "Enter a valid Cabinet server URL".to_string())?;
        match parsed.scheme() {
            "http" | "https" if parsed.host_str().is_some() => Ok(candidate.to_string()),
            _ => Err("Server URL must use http:// or https:// and include a hostname".to_string()),
        }
    }

    fn http() -> Result<Client, String> {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("Cabinet-Desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())
    }

    pub fn login(server_url: &str, username: &str, password: &str) -> Result<(Self, User), String> {
        let base_url = Self::normalize_server_url(server_url)?;
        let http = Self::http()?;
        let response = http
            .post(format!("{base_url}/api/auth/login"))
            .json(&json!({ "username": username, "password": password }))
            .send()
            .map_err(|e| e.to_string())?;
        let logged: LoginResponse = decode(response)?;
        let client = Self { base_url, token: logged.token, http };
        let user = client.me()?;
        Ok((client, user))
    }

    pub fn from_token(server_url: &str, token: String) -> Result<Self, String> {
        Ok(Self {
            base_url: Self::normalize_server_url(server_url)?,
            token,
            http: Self::http()?,
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
    }

    pub fn me(&self) -> Result<User, String> {
        let response = self.request(Method::GET, "/api/auth/me").send().map_err(|e| e.to_string())?;
        Ok(decode::<MeResponse>(response)?.user)
    }

    pub fn logout(&self) {
        let _ = self.request(Method::POST, "/api/auth/logout").send();
    }

    pub fn list_files(&self) -> Result<Vec<CabinetFile>, String> {
        Ok(decode::<FilesResponse>(
            self.request(Method::GET, "/api/files").send().map_err(|e| e.to_string())?
        )?.files)
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>, String> {
        Ok(decode::<FoldersResponse>(
            self.request(Method::GET, "/api/folders").send().map_err(|e| e.to_string())?
        )?.folders)
    }

    pub fn create_folder(&self, name: &str, parent_id: Option<&str>) -> Result<Folder, String> {
        let response = self
            .request(Method::POST, "/api/folders")
            .json(&json!({ "name": name, "parentId": parent_id }))
            .send().map_err(|e| e.to_string())?;
        Ok(decode::<FolderResponse>(response)?.folder)
    }

    pub fn delete_folder(&self, id: &str) -> Result<(), String> {
        unit(self.request(Method::DELETE, &format!("/api/folders/{id}")).send().map_err(|e| e.to_string())?)
    }

    pub fn upload_file(&self, path: &Path, parent_id: Option<&str>) -> Result<CabinetFile, String> {
        let name = path.file_name().and_then(|x| x.to_str())
            .ok_or_else(|| "Selected file has an invalid name".to_string())?;
        let file = File::open(path).map_err(|e| e.to_string())?;
        let part = multipart::Part::reader(file).file_name(name.to_string());
        let mut form = multipart::Form::new().part("file", part);
        if let Some(parent_id) = parent_id {
            form = form.text("parentId", parent_id.to_string());
        }
        let response = self.request(Method::POST, "/api/files")
            .multipart(form).send().map_err(|e| e.to_string())?;
        Ok(decode::<FileResponse>(response)?.file)
    }

    pub fn download_file(&self, id: &str, destination: &Path) -> Result<(), String> {
        let mut response = self.request(Method::GET, &format!("/api/files/{id}/content?download=true"))
            .send().map_err(|e| e.to_string())?;
        ensure_success(&mut response)?;
        let mut output = File::create(destination).map_err(|e| e.to_string())?;
        io::copy(&mut response, &mut output).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn rename_file(&self, id: &str, name: &str) -> Result<CabinetFile, String> {
        let response = self.request(Method::PATCH, &format!("/api/files/{id}"))
            .json(&json!({ "name": name }))
            .send().map_err(|e| e.to_string())?;
        Ok(decode::<FileResponse>(response)?.file)
    }

    pub fn move_file(&self, id: &str, parent_id: Option<&str>) -> Result<CabinetFile, String> {
        let response = self.request(Method::PATCH, &format!("/api/files/{id}"))
            .json(&json!({ "parentId": parent_id }))
            .send().map_err(|e| e.to_string())?;
        Ok(decode::<FileResponse>(response)?.file)
    }

    pub fn delete_file(&self, id: &str) -> Result<(), String> {
        unit(self.request(Method::DELETE, &format!("/api/files/{id}")).send().map_err(|e| e.to_string())?)
    }

    pub fn share_with_user(&self, id: &str, username: &str) -> Result<(), String> {
        unit(self.request(Method::POST, &format!("/api/files/{id}/share"))
            .json(&json!({ "username": username }))
            .send().map_err(|e| e.to_string())?)
    }

    pub fn create_public_share(&self, file_id: &str) -> Result<String, String> {
        let response = self.request(Method::POST, "/api/shares")
            .json(&json!({
                "fileId": file_id,
                "password": Value::Null,
                "expiresAt": Value::Null,
                "downloadLimit": Value::Null
            }))
            .send().map_err(|e| e.to_string())?;
        let result: ShareResponse = decode(response)?;
        Ok(format!("{}{}", self.base_url, result.link))
    }

    pub fn list_shares(&self) -> Result<Vec<Share>, String> {
        Ok(decode::<SharesResponse>(
            self.request(Method::GET, "/api/shares").send().map_err(|e| e.to_string())?
        )?.shares)
    }

    pub fn revoke_share(&self, id: &str) -> Result<(), String> {
        unit(self.request(Method::DELETE, &format!("/api/shares/{id}")).send().map_err(|e| e.to_string())?)
    }

    pub fn admin_stats(&self) -> Result<AdminStats, String> {
        decode(self.request(Method::GET, "/api/admin/stats").send().map_err(|e| e.to_string())?)
    }

    pub fn admin_users(&self) -> Result<Vec<AdminUser>, String> {
        Ok(decode::<AdminUsersResponse>(
            self.request(Method::GET, "/api/admin/users").send().map_err(|e| e.to_string())?
        )?.users)
    }

    pub fn admin_shares(&self) -> Result<Vec<AdminShare>, String> {
        Ok(decode::<AdminSharesResponse>(
            self.request(Method::GET, "/api/admin/shares").send().map_err(|e| e.to_string())?
        )?.shares)
    }

    pub fn admin_logs(&self) -> Result<String, String> {
        let mut response = self.request(Method::GET, "/api/admin/logs").send().map_err(|e| e.to_string())?;
        ensure_success(&mut response)?;
        response.text().map_err(|e| e.to_string())
    }

    pub fn is_unauthorized(error: &str) -> bool {
        error.contains("401") || error.to_ascii_lowercase().contains("unauthorized")
    }
}

fn ensure_success(response: &mut Response) -> Result<(), String> {
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body: Value = response.json().unwrap_or_else(|_| json!({}));
    Err(body.get("error").and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Cabinet API returned {status}")))
}

fn unit(mut response: Response) -> Result<(), String> {
    ensure_success(&mut response)
}

fn decode<T: DeserializeOwned>(mut response: Response) -> Result<T, String> {
    ensure_success(&mut response)?;
    response.json::<T>().map_err(|e| e.to_string())
}

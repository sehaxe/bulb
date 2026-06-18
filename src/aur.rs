use crate::error::{BulbError, Result};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AurSearchResult {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "URL")]
    pub url: Option<String>,
    #[serde(rename = "NumVotes")]
    pub num_votes: Option<u64>,
    #[serde(rename = "Popularity")]
    pub popularity: Option<f64>,
    #[serde(rename = "OutOfDate")]
    pub out_of_date: Option<u64>,
    #[serde(rename = "Maintainer")]
    pub maintainer: Option<String>,
    #[serde(rename = "Submitter")]
    pub submitter: Option<String>,
    #[serde(rename = "FirstSubmitted")]
    pub first_submitted: Option<u64>,
    #[serde(rename = "License")]
    pub license: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AurInfoResult {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "URL")]
    pub url: Option<String>,
    #[serde(rename = "License")]
    pub license: Option<String>,
    #[serde(rename = "Depends")]
    pub depends: Option<Vec<String>>,
    #[serde(rename = "MakeDepends")]
    pub make_depends: Option<Vec<String>>,
    #[serde(rename = "Provides")]
    pub provides: Option<Vec<String>>,
    #[serde(rename = "Conflicts")]
    pub conflicts: Option<Vec<String>>,
    #[serde(rename = "Replaces")]
    pub replaces: Option<Vec<String>>,
    #[serde(rename = "OutOfDate")]
    pub out_of_date: Option<u64>,
    #[serde(rename = "Maintainer")]
    pub maintainer: Option<String>,
    #[serde(rename = "URLPath")]
    pub url_path: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct AurResponse<T> {
    _resultcount: u64,
    results: Vec<T>,
    #[serde(rename = "type")]
    result_type: String,
}

pub async fn search_aur(query: &str) -> Result<Vec<AurSearchResult>> {
    let url = format!(
        "https://aur.archlinux.org/rpc.php?v5&type=search&arg={}",
        urlencoding::encode(query)
    );

    let client = reqwest::Client::builder()
        .user_agent("bulb/0.1")
        .build()
        .map_err(|e| BulbError::Config(format!("http client: {e}")))?;

    let response = client.get(&url).send().await
        .map_err(|e| BulbError::Config(format!("AUR search failed: {e}")))?;

    if !response.status().is_success() {
        return Err(BulbError::Config(format!(
            "AUR search failed: HTTP {}",
            response.status()
        )));
    }

    let text = response.text().await
        .map_err(|e| BulbError::Config(format!("AUR read failed: {e}")))?;

    let parsed: AurResponse<AurSearchResult> = serde_json::from_str(&text)
        .map_err(|e| BulbError::Config(format!("AUR parse failed: {e}")))?;

    if parsed.result_type == "error" {
        return Err(BulbError::Config(format!("AUR returned error: {}", text)));
    }

    Ok(parsed.results)
}

pub async fn get_aur_info(pkgname: &str) -> Result<AurInfoResult> {
    let url = format!(
        "https://aur.archlinux.org/rpc.php?v5&type=info&arg[]={}",
        urlencoding::encode(pkgname)
    );

    let client = reqwest::Client::builder()
        .user_agent("bulb/0.1")
        .build()
        .map_err(|e| BulbError::Config(format!("http client: {e}")))?;

    let response = client.get(&url).send().await
        .map_err(|e| BulbError::Config(format!("AUR info failed: {e}")))?;

    if !response.status().is_success() {
        return Err(BulbError::Config(format!(
            "AUR info failed: HTTP {}",
            response.status()
        )));
    }

    let text = response.text().await
        .map_err(|e| BulbError::Config(format!("AUR read failed: {e}")))?;

    let parsed: AurResponse<AurInfoResult> = serde_json::from_str(&text)
        .map_err(|e| BulbError::Config(format!("AUR parse failed: {e}")))?;

    parsed.results.into_iter().next()
        .ok_or_else(|| BulbError::PackageNotFound(format!("{} not found in AUR", pkgname)))
}

pub fn aur_download_url(pkgname: &str) -> String {
    format!("https://aur.archlinux.org/cgit/aur.git/snapshot/{}.tar.gz", pkgname)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_url_format() {
        let url = aur_download_url("firefox-bin");
        assert_eq!(url, "https://aur.archlinux.org/cgit/aur.git/snapshot/firefox-bin.tar.gz");
    }
}

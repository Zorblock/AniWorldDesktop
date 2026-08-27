use std::time::Duration;

const ANILIST_API_URL: &str = "https://graphql.anilist.co";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_RESPONSE_SIZE: u64 = 512 * 1024;
const COVER_QUERY: &str = r#"
query ($search: String!) {
  Media(search: $search, type: ANIME) {
    coverImage {
      extraLarge
      large
    }
  }
}
"#;

pub fn fetch_cover_url(title: &str) -> Result<Option<String>, String> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(None);
    }

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let request_body = serde_json::json!({
        "query": COVER_QUERY,
        "variables": { "search": title }
    })
    .to_string();

    let mut response = agent
        .post(ANILIST_API_URL)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "AniWorldDesktop/0.1")
        .send(request_body)
        .map_err(|error| error.to_string())?;
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_SIZE)
        .read_to_string()
        .map_err(|error| error.to_string())?;

    cover_url_from_response(&body)
}

pub fn is_anilist_cover_url(candidate: &str) -> bool {
    let Ok(url) = tauri::Url::parse(candidate) else {
        return false;
    };
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host == "anilist.co" || host.ends_with(".anilist.co"))
}

fn cover_url_from_response(response: &str) -> Result<Option<String>, String> {
    let response: serde_json::Value =
        serde_json::from_str(response).map_err(|error| error.to_string())?;
    if let Some(errors) = response.get("errors") {
        return Err(format!("AniList returned an API error: {errors}"));
    }

    let cover = &response["data"]["Media"]["coverImage"];
    Ok(cover["extraLarge"]
        .as_str()
        .or_else(|| cover["large"].as_str())
        .filter(|url| is_anilist_cover_url(url))
        .map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_extra_large_anilist_cover() {
        let response = r#"{
          "data": { "Media": { "coverImage": {
            "extraLarge": "https://s4.anilist.co/file/anilistcdn/large.png",
            "large": "https://s4.anilist.co/file/anilistcdn/medium.png"
          } } }
        }"#;

        assert_eq!(
            cover_url_from_response(response).unwrap().as_deref(),
            Some("https://s4.anilist.co/file/anilistcdn/large.png")
        );
    }

    #[test]
    fn rejects_untrusted_cover_hosts() {
        let response = r#"{
          "data": { "Media": { "coverImage": {
            "extraLarge": "https://example.com/fake-cover.png"
          } } }
        }"#;

        assert_eq!(cover_url_from_response(response).unwrap(), None);
    }
}

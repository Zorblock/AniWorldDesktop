use adblock::{
    lists::{FilterSet, ParseOptions},
    request::Request,
    Engine,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
const EASYLIST_URL: &str = "https://easylist.to/easylist/easylist.txt";
const CACHE_MAX_AGE: Duration = Duration::from_secs(4 * 24 * 60 * 60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_FILTER_LIST_SIZE: usize = 10 * 1024 * 1024;

const FALLBACK_FILTERS: &str = r#"[Adblock Plus 2.0]
! Title: AniWorld Desktop fallback filters
||2mdn.net^
||adnxs.com^
||adsterra.com^
||amazon-adsystem.com^
||doubleclick.net^
||exoclick.com^
||googleadservices.com^
||googlesyndication.com^
||juicyads.com^
||onclickads.net^
||outbrain.com^
||popads.net^
||popcash.net^
||propellerads.com^
||scorecardresearch.com^
||taboola.com^
||trafficjunky.net^
/ads.js$script,third-party
/popunder.js$script
"#;

const COSMETIC_FALLBACK_CSS: &str = r#"
.adsbygoogle,
.ad-banner,
.ad-container,
.advertisement,
[id^="google_ads_"],
[id^="ad-container"],
[class~="advertisement"],
iframe[src*="doubleclick.net"],
iframe[src*="googlesyndication.com"] {
  display: none !important;
}
"#;

pub struct AdBlocker {
    engine: Engine,
}

impl AdBlocker {
    pub fn load(user_data_directory: &Path) -> Arc<Self> {
        let list = load_filter_list(user_data_directory);
        let mut filter_set = FilterSet::new(false);
        filter_set.add_filter_list(FALLBACK_FILTERS.to_owned(), ParseOptions::default());
        filter_set.add_filter_list(list, ParseOptions::default());

        Arc::new(Self {
            engine: Engine::new_with_filter_set(filter_set),
        })
    }

    pub fn blocks(&self, url: &str, source_url: &str, request_type: &str, method: &str) -> bool {
        Request::new(url, source_url, request_type, method)
            .map(|request| self.engine.check_network_request(&request).should_block())
            .unwrap_or(false)
    }

    pub(crate) fn cosmetic_css(&self) -> String {
        let mut selectors: Vec<_> = self
            .engine
            .url_cosmetic_resources("https://aniworld.to/")
            .hide_selectors
            .into_iter()
            .collect();
        selectors.sort_unstable();

        let mut css = String::from(COSMETIC_FALLBACK_CSS);
        for selector in selectors {
            css.push_str(&selector);
            css.push_str(" { display: none !important; }\n");
        }
        css
    }
}

fn cache_path(user_data_directory: &Path) -> PathBuf {
    user_data_directory.join("adblock").join("easylist.txt")
}

fn cache_is_fresh(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age <= CACHE_MAX_AGE)
}

fn looks_like_filter_list(contents: &str) -> bool {
    contents.len() >= 100_000
        && contents.len() <= MAX_FILTER_LIST_SIZE
        && contents
            .lines()
            .take(20)
            .any(|line| line.starts_with("[Adblock Plus") || line.starts_with("! Title: EasyList"))
}

fn read_valid_cache(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .filter(|contents| looks_like_filter_list(contents))
}

fn download_filter_list() -> Result<String, Box<dyn std::error::Error>> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let contents = agent
        .get(EASYLIST_URL)
        .header("User-Agent", "AniWorldDesktop/0.1")
        .call()?
        .body_mut()
        .with_config()
        .limit(MAX_FILTER_LIST_SIZE as u64)
        .read_to_string()?;

    if looks_like_filter_list(&contents) {
        Ok(contents)
    } else {
        Err("The EasyList response is not a valid filter list".into())
    }
}

fn load_filter_list(user_data_directory: &Path) -> String {
    let path = cache_path(user_data_directory);
    let cached = read_valid_cache(&path);

    if cache_is_fresh(&path) {
        if let Some(contents) = cached {
            return contents;
        }
    }

    match download_filter_list() {
        Ok(contents) => {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(error) = fs::write(&path, &contents) {
                eprintln!("Could not save the EasyList cache: {error}");
            }
            contents
        }
        Err(error) => {
            eprintln!("Could not update EasyList: {error}");
            cached.unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_blocks_a_known_ad_domain() {
        let mut filter_set = FilterSet::new(false);
        filter_set.add_filter_list(FALLBACK_FILTERS.to_owned(), ParseOptions::default());
        let blocker = AdBlocker {
            engine: Engine::new_with_filter_set(filter_set),
        };

        assert!(blocker.blocks(
            "https://securepubads.g.doubleclick.net/tag.js",
            "https://aniworld.to/",
            "script",
            "GET"
        ));
        assert!(!blocker.blocks(
            "https://aniworld.to/",
            "https://aniworld.to/",
            "document",
            "GET"
        ));
    }

    #[test]
    fn rejects_small_or_unrelated_cache_files() {
        assert!(!looks_like_filter_list(
            "[Adblock Plus 2.0]\\n||example.test^"
        ));
        assert!(!looks_like_filter_list(&"x".repeat(100_000)));
    }
}

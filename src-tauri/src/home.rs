//! Live data for the Home page from free public sources: Steam's news API
//! (developer announcements only), CheapShark's deals API, and Steam's store
//! front page lists. Everything is cached to disk so Home opens instantly.

use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct News {
    app: u32,
    title: String,
    url: String,
    text: String,
    date: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    t: String,
    app: u32,
    sale: f64,
    was: f64,
    off: u32,
    rating: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    t: String,
    app: u32,
    price: f64,
    off: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Home {
    fetched_at: u64,
    news: Vec<News>,
    deals: Vec<Deal>,
    top: Vec<Ranked>,
    new_releases: Vec<Ranked>,
    /// Sources that failed this time (the UI keeps the previous data for those).
    failed: Vec<&'static str>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("HotSwap/0.1 (desktop game companion)")
        .build()
}

fn get(agent: &ureq::Agent, url: &str) -> Option<Value> {
    agent.get(url).call().ok()?.into_json().ok()
}

/// Strips BBCode, HTML tags and Steam image placeholders from an announcement body.
fn plain(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            // A removed tag still separates words ("[*]Added" -> " Added").
            '[' | '<' | '{' => {
                depth += 1;
                out.push(' ');
            }
            ']' | '>' | '}' if depth > 0 => depth -= 1,
            '\\' => {}
            _ if depth == 0 => out.push(if c.is_whitespace() { ' ' } else { c }),
            _ => {}
        }
    }
    let squeezed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if squeezed.chars().count() <= max {
        return squeezed;
    }
    let cut: String = squeezed.chars().take(max).collect();
    format!("{}…", cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a))
}

pub fn fetch(app_ids: &[u32]) -> Home {
    let agent = agent();
    let mut failed = Vec::new();

    // Developer announcements only: they're in the game's own words and link back to Steam.
    let mut news = Vec::new();
    let mut news_ok = false;
    for &app in app_ids.iter().take(8) {
        let url = format!(
            "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid={app}&count=3&maxlength=600&feeds=steam_community_announcements"
        );
        let Some(v) = get(&agent, &url) else { continue };
        news_ok = true;
        for item in v["appnews"]["newsitems"].as_array().into_iter().flatten() {
            news.push(News {
                app,
                title: item["title"].as_str().unwrap_or_default().trim().to_string(),
                url: item["url"].as_str().unwrap_or_default().to_string(),
                text: plain(item["contents"].as_str().unwrap_or_default(), 180),
                date: item["date"].as_u64().unwrap_or(0),
            });
        }
    }
    news.retain(|n| !n.title.is_empty());
    news.sort_by(|a, b| b.date.cmp(&a.date));
    news.truncate(9);
    if !news_ok && !app_ids.is_empty() {
        failed.push("news");
    }

    let mut deals = Vec::new();
    match get(&agent, "https://www.cheapshark.com/api/1.0/deals?storeID=1&sortBy=Deal%20Rating&pageSize=20&onSale=1&AAA=0") {
        Some(Value::Array(list)) => {
            let num = |v: &Value| v.as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
            for d in list {
                let app = d["steamAppID"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                if app == 0 || deals.iter().any(|x: &Deal| x.app == app) {
                    continue;
                }
                deals.push(Deal {
                    t: d["title"].as_str().unwrap_or_default().to_string(),
                    app,
                    sale: num(&d["salePrice"]),
                    was: num(&d["normalPrice"]),
                    off: num(&d["savings"]).round() as u32,
                    rating: d["steamRatingText"].as_str().unwrap_or_default().to_string(),
                });
            }
            deals.truncate(9);
        }
        _ => failed.push("deals"),
    }

    let (mut top, mut new_releases) = (Vec::new(), Vec::new());
    match get(&agent, "https://store.steampowered.com/api/featuredcategories?cc=us&l=en") {
        Some(v) => {
            let list = |key: &str, max: usize| {
                let mut out: Vec<Ranked> = Vec::new();
                for i in v[key]["items"].as_array().into_iter().flatten() {
                    let app = i["id"].as_u64().unwrap_or(0) as u32;
                    if app == 0 || out.iter().any(|x| x.app == app) {
                        continue;
                    }
                    out.push(Ranked {
                        t: i["name"].as_str().unwrap_or_default().to_string(),
                        app,
                        price: i["final_price"].as_f64().unwrap_or(0.0) / 100.0,
                        off: i["discount_percent"].as_u64().unwrap_or(0) as u32,
                    });
                }
                out.truncate(max);
                out
            };
            top = list("top_sellers", 7);
            new_releases = list("new_releases", 5);
        }
        None => failed.push("store"),
    }

    Home {
        fetched_at: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        news,
        deals,
        top,
        new_releases,
        failed,
    }
}

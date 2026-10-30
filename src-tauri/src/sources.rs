//! Custom sources for Home page panels: fetches a URL and works out what it is. RSS and Atom feeds
//! (and websites that announce a feed in their page header) become a list of links; a JSON list in
//! HotSwap's simple format does too; any other website becomes a web panel, if the site allows
//! being shown inside apps. Feeds are read with a small reader here, no XML library.

use serde_json::{json, Value};
use std::{io::Read, time::Duration};

const LIMIT: u64 = 5 << 20;

fn get(url: &str) -> Result<(String, String, String, String), String> {
    let r = ureq::get(url)
        .timeout(Duration::from_secs(15))
        .set("User-Agent", "HotSwap/0.3 (desktop game companion)")
        .call()
        .map_err(|e| format!("Couldn't open that address: {e}"))?;
    let ctype = r.content_type().to_lowercase();
    let xfo = r.header("x-frame-options").unwrap_or("").to_lowercase();
    let csp = r.header("content-security-policy").unwrap_or("").to_lowercase();
    let mut body = String::new();
    r.into_reader().take(LIMIT).read_to_string(&mut body).map_err(|e| e.to_string())?;
    Ok((ctype, xfo, csp, body))
}

/// Fetches `url` and returns { kind: "list", title, items } or { kind: "web", url, framable }.
pub fn fetch(url: &str) -> Result<Value, String> {
    let url = url.trim();
    let url = if url.starts_with("http://") || url.starts_with("https://") { url.to_string() } else { format!("https://{url}") };
    let (ctype, xfo, csp, body) = get(&url)?;
    let head = body.trim_start().get(..400).unwrap_or(body.trim_start()).to_lowercase();
    // 1. A feed.
    if head.contains("<rss") || head.contains("<feed") || head.contains("<rdf:rdf") || ctype.contains("rss") || ctype.contains("atom") {
        return Ok(feed(&body, &url));
    }
    // 2. HotSwap's JSON format: { "title": "...", "items": [{ "title", "url", "image", "text", "date" }] }.
    if ctype.contains("json") || head.starts_with('{') {
        let v: Value = serde_json::from_str(&body).map_err(|_| "That address returns JSON, but not in HotSwap's list format".to_string())?;
        let items = v["items"].as_array().ok_or("That JSON has no \"items\" list")?;
        let items: Vec<Value> = items.iter().take(40).map(|i| json!({ "title": i["title"], "url": i["url"], "image": i["image"], "text": i["text"], "date": i["date"] })).collect();
        return Ok(json!({ "kind": "list", "title": v["title"].as_str().unwrap_or(""), "items": items }));
    }
    // 3. A website that links its feed.
    if let Some(f) = feed_link(&body, &url) {
        if let Ok((_, _, _, fb)) = get(&f) {
            let mut v = feed(&fb, &f);
            v["feed"] = json!(f);
            return Ok(v);
        }
    }
    // 4. A website, shown as it is if it allows that.
    let framable = !(xfo.contains("deny") || xfo.contains("sameorigin") || (csp.contains("frame-ancestors") && !csp.contains("frame-ancestors *")));
    Ok(json!({ "kind": "web", "url": url, "framable": framable, "title": tag(&body, "title").map(|t| clean(&t)).unwrap_or_default() }))
}

/// The items of an RSS or Atom feed.
fn feed(xml: &str, base: &str) -> Value {
    let channel_title = tag(xml, "title").map(|t| clean(&t)).unwrap_or_default();
    let mut items = Vec::new();
    for block in blocks(xml, "item").into_iter().chain(blocks(xml, "entry")).take(40) {
        let title = tag(block, "title").map(|t| clean(&t)).unwrap_or_default();
        let link = attr_of(block, "link", "href").or_else(|| tag(block, "link").map(|t| clean(&t))).unwrap_or_default();
        let date = ["pubDate", "published", "updated", "dc:date"].iter().find_map(|t| tag(block, t)).map(|t| clean(&t)).unwrap_or_default();
        let raw = ["description", "summary", "content", "content:encoded"].iter().find_map(|t| tag(block, t)).unwrap_or_default();
        let image = attr_of(block, "media:thumbnail", "url")
            .or_else(|| attr_of(block, "media:content", "url"))
            .or_else(|| attr_of(block, "enclosure", "url").filter(|u| is_image(u)))
            .or_else(|| attr_of(&unescape(&strip_cdata(&raw)), "img", "src"))
            .unwrap_or_default();
        let text: String = clean(&raw).chars().take(220).collect();
        items.push(json!({ "title": title, "url": absolute(&link, base), "image": absolute(&image, base), "text": text, "date": date }));
    }
    json!({ "kind": "list", "title": channel_title, "items": items })
}

fn is_image(u: &str) -> bool {
    let u = u.to_lowercase();
    [".jpg", ".jpeg", ".png", ".webp", ".gif"].iter().any(|e| u.contains(e))
}

/// The contents of every <name ...>...</name> element.
fn blocks<'a>(s: &'a str, name: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{name}"), format!("</{name}>"));
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = s[from..].find(&open).map(|i| i + from) {
        let after = s[i + open.len()..].chars().next();
        if !matches!(after, Some('>' | ' ' | '\n' | '\r' | '\t')) {
            from = i + open.len();
            continue;
        }
        let Some(gt) = s[i..].find('>').map(|g| g + i) else { break };
        let Some(end) = s[gt..].find(&close).map(|e| e + gt) else { break };
        out.push(&s[gt + 1..end]);
        from = end + close.len();
    }
    out
}

/// The text of the first <name>...</name>.
fn tag(s: &str, name: &str) -> Option<String> {
    blocks(s, name).first().map(|t| t.to_string())
}

/// An attribute of the first <name ...> tag.
fn attr_of(s: &str, name: &str, attr: &str) -> Option<String> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(i) = s[from..].find(&open).map(|i| i + from) {
        let end = s[i..].find('>').map(|g| g + i)?;
        let t = &s[i..end];
        from = end;
        for q in ['"', '\''] {
            let key = format!("{attr}={q}");
            if let Some(a) = t.find(&key) {
                let rest = &t[a + key.len()..];
                if let Some(b) = rest.find(q) {
                    return Some(unescape(&rest[..b]));
                }
            }
        }
    }
    None
}

/// A feed link a page announces in its header (<link rel="alternate" type="application/rss+xml">).
fn feed_link(html: &str, base: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<link").map(|i| i + from) {
        let end = lower[i..].find('>').map(|g| g + i)?;
        let t = &lower[i..end];
        from = end;
        if t.contains("alternate") && (t.contains("application/rss+xml") || t.contains("application/atom+xml")) {
            let href = attr_of(&html[i..=end], "link", "href")?;
            return Some(absolute(&href, base));
        }
    }
    None
}

fn strip_cdata(s: &str) -> String {
    s.replace("<![CDATA[", "").replace("]]>", "")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&nbsp;", " ").replace("&#8217;", "’").replace("&#8216;", "‘").replace("&#8220;", "“").replace("&#8221;", "”").replace("&#8211;", "–").replace("&#8212;", "—").replace("&amp;", "&")
}

/// Plain text: CDATA, tags and entities removed, spaces collapsed.
fn clean(s: &str) -> String {
    let s = unescape(&strip_cdata(s));
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Makes a link absolute against the page it came from.
fn absolute(link: &str, base: &str) -> String {
    let link = link.trim();
    if link.is_empty() || link.starts_with("http://") || link.starts_with("https://") {
        return link.to_string();
    }
    let origin_end = base.find("://").map(|i| i + 3).and_then(|s| base[s..].find('/').map(|j| j + s)).unwrap_or(base.len());
    if let Some(rest) = link.strip_prefix("//") {
        return format!("https://{rest}");
    }
    if link.starts_with('/') {
        return format!("{}{link}", &base[..origin_end]);
    }
    format!("{}/{link}", base.trim_end_matches('/'))
}

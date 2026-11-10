//! coffer-server: serves the Shelf over loopback HTTP.
//!
//! Routes:
//!   GET /                    the chrome: nav bar, address field, tabs
//!   GET /shelf               shelf home: book list + search box
//!   GET /book/<id>           redirect to the book's main entry
//!   GET /book/<id>/<path>    one item out of a book, with its MIME type
//!   GET /search?q=<query>    one search across every book aboard
//!   GET /catalog?q=&lang=&c= the Kiwix OPDS catalog, organized by
//!                            category and publisher, when a wire exists
//!   GET /acquire?url=&name=  start fetching a book onto the shelf
//!   GET /acquire-progress    watch the fetch come down
//!   GET /api/acquire-status  the fetch as JSON
//!   GET /api/books           the shelf as JSON
//!   GET /api/search?q=       search results as JSON
//!
//! The webview window loads this server; nothing in Coffer renders around
//! it. That keeps the engine borrowed and swappable (determination 1).
//! Requests are handled on threads: a long download never blocks reading.

use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tiny_http::{Header, Response, Server};
use coffer_core::Shelf;

mod epub;
mod pmtiles;
mod sync;
use epub::EpubBook;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn json_esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    for pair in q.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(url_decode(v));
            }
        } else if pair == key {
            return Some(String::new());
        }
    }
    None
}

fn html_response(body: String) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(body).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
    )
}

fn json_response(body: String) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
}

/// The chrome: one page that carries navigation for every surface.
/// Plain script, no build step, nothing fetched. Tabs are saved states:
/// one live view, and switching a tab swaps the address it shows.
const CHROME: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Coffer</title>
<style>
html,body{margin:0;height:100%;font-family:system-ui,sans-serif;background:#fff;color:#111}
#bar{display:flex;gap:6px;align-items:center;padding:6px 8px;background:#eee;border-bottom:1px solid #ccc}
#bar button{font-size:15px;padding:2px 9px;cursor:pointer}
#addr{flex:1;font-size:15px;padding:4px 8px}
#tabs{display:flex;gap:4px;align-items:center;padding:4px 8px;background:#f7f7f7;border-bottom:1px solid #ddd;flex-wrap:wrap}
.tab{font-size:13px;padding:3px 8px;cursor:pointer;border:1px solid #ccc;background:#fff;max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.tab.on{background:#dde7ff;border-color:#88a}
.tabx{font-size:12px;padding:3px 6px;cursor:pointer;border:1px solid #ddd;background:#fff}
#view{width:100%;height:calc(100% - 84px);border:0;display:block}
</style></head>
<body>
<div id="bar">
<button id="back" title="Back">&#8249;</button>
<button id="fwd" title="Forward">&#8250;</button>
<button id="reload" title="Reload">&#8635;</button>
<input id="addr" placeholder="Search the Shelf, or a path like /book/0">
<button id="go">Go</button>
<button id="shelfbtn">Shelf</button>
<button id="catbtn">Catalog</button>
<button id="markbtn" title="Bookmark this page">&#9734;</button>
<button id="marksbtn">Marks</button>
<button id="histbtn">History</button>
<button id="mapsbtn">Maps</button>
<button id="syncbtn">Sync</button>
</div>
<div id="tabs"></div>
<iframe id="view" src="/shelf"></iframe>
<script>
var tabs=[{url:'/shelf',title:'Shelf'}], active=0;
var view=document.getElementById('view'), addr=document.getElementById('addr');
function renderTabs(){
  var t=document.getElementById('tabs'); t.innerHTML='';
  tabs.forEach(function(tab,i){
    var b=document.createElement('button');
    b.textContent=tab.title||tab.url; b.className='tab'+(i===active?' on':'');
    b.onclick=function(){switchTab(i)};
    var x=document.createElement('button');
    x.textContent='\u00d7'; x.className='tabx'; x.title='Close tab';
    x.onclick=function(ev){ev.stopPropagation(); closeTab(i)};
    t.appendChild(b); t.appendChild(x);
  });
  var n=document.createElement('button');
  n.textContent='+'; n.className='tabx'; n.title='New tab';
  n.onclick=function(){newTab()};
  t.appendChild(n);
}
function current(){
  try { return view.contentWindow.location.pathname + view.contentWindow.location.search; }
  catch(e){ return tabs[active].url; }
}
function go(url){ tabs[active].url=url; view.src=url; }
function switchTab(i){ tabs[active].url=current(); active=i; view.src=tabs[i].url; renderTabs(); }
function newTab(){ tabs.push({url:'/shelf',title:'Shelf'}); switchTab(tabs.length-1); }
function closeTab(i){
  if(tabs.length===1) return;
  tabs.splice(i,1);
  if(active>=tabs.length) active=tabs.length-1; else if(i<active) active--;
  view.src=tabs[active].url; renderTabs();
}
view.addEventListener('load', function(){
  var u=current(); tabs[active].url=u; addr.value=u;
  try { tabs[active].title=view.contentDocument.title||u; } catch(e){}
  renderTabs();
  fetch('/api/history?url=' + encodeURIComponent(u) + '&title=' + encodeURIComponent(tabs[active].title || u)).catch(function(){});
  fetch('/api/bookmark-check?url=' + encodeURIComponent(u)).then(function(r){ return r.json(); }).then(function(j){
    document.getElementById('markbtn').innerHTML = j.bookmarked ? '&#9733;' : '&#9734;';
  }).catch(function(){});
});
function submitAddr(){
  var v=addr.value.trim(); if(!v) return;
  if(v.indexOf('http://127.0.0.1')===0){ v=v.slice(16); }
  if(v.charAt(0)==='/'){ go(v); }
  else if(v.indexOf('/')>=0 && v.indexOf(' ')<0){ go('/'+v); }
  else { go('/search?q='+encodeURIComponent(v)); }
}
addr.addEventListener('keydown', function(e){ if(e.key==='Enter') submitAddr(); });
document.getElementById('go').onclick=submitAddr;
document.getElementById('back').onclick=function(){ view.contentWindow.history.back(); };
document.getElementById('fwd').onclick=function(){ view.contentWindow.history.forward(); };
document.getElementById('reload').onclick=function(){ try{ view.contentWindow.location.reload(); }catch(e){ view.src=current(); } };
document.getElementById('shelfbtn').onclick=function(){ go('/shelf'); };
document.getElementById('catbtn').onclick=function(){ go('/catalog'); };
document.getElementById('mapsbtn').onclick=function(){ go('/maps'); };
document.getElementById('syncbtn').onclick=function(){ go('/sync'); };
document.getElementById('markbtn').onclick=function(){
  var u=current();
  fetch('/api/bookmark?url=' + encodeURIComponent(u) + '&title=' + encodeURIComponent(tabs[active].title || u)).then(function(r){ return r.json(); }).then(function(j){
    document.getElementById('markbtn').innerHTML = j.bookmarked ? '&#9733;' : '&#9734;';
  }).catch(function(){});
};
document.getElementById('marksbtn').onclick=function(){ go('/bookmarks'); };
document.getElementById('histbtn').onclick=function(){ go('/history'); };
renderTabs();
</script>
</body></html>"##;

fn shelf_page(shelf: &Shelf, epubs: &[EpubBook]) -> String {
    let mut items = String::new();
    for b in &shelf.books {
        items.push_str(&format!(
            "<p class=\"item\">- <a href=\"/book/{}\">{}</a> <span class=\"meta\">{} articles, {}</span></p>\n",
            b.id,
            esc(&b.title),
            b.article_count,
            esc(&b.file_name)
        ));
    }
    if shelf.books.is_empty() && epubs.is_empty() {
        items = "<p>No books aboard yet. Add .zim or .epub files to the shelf directory, or fetch some from the <a href=\"/catalog\">catalog</a>.</p>".to_string();
    }
    if !epubs.is_empty() {
        items.push_str("<h2>Books (EPUB)</h2>\n");
        for e in epubs {
            items.push_str(&format!(
                "<p class=\"item\">- <a href=\"/epub/{}/\">{}</a> <span class=\"meta\">{} chapters, {}</span></p>\n",
                url_encode(&e.file_name),
                esc(&e.title),
                e.chapters.len(),
                esc(&e.file_name)
            ));
        }
    }
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Shelf</title></head>\n\
         <body><h1>The Shelf</h1>\n\
         <form action=\"/search\" method=\"get\"><input name=\"q\" size=\"40\" \
         placeholder=\"Search everything aboard\"> <input type=\"submit\" value=\"Search\"></form>\n\
         <h2>Aboard</h2>\n{}\
         <p><a href=\"/catalog\">Catalog: fetch more books</a></p></body></html>",
        items
    )
}

struct Hit {
    book: usize,
    path: String,
    title: String,
    score: f64,
}

/// One search across every book aboard. Each book answers in its own
/// ranking (full text where it has an index, title suggestions where it
/// does not); the lists are merged by reciprocal rank fusion so a book's
/// best answer outranks another book's twentieth. Rank inside the book
/// is the only signal the books share, which is exactly what RRF needs.
fn federated_search(shelf: &Shelf, query: &str) -> Vec<Hit> {
    const RRF_K: f64 = 60.0;
    let mut hits: Vec<Hit> = Vec::new();
    for b in &shelf.books {
        let results = match b.search(query, 20) {
            Ok(h) => h,
            Err(_) => b.suggest(query, 20).unwrap_or_default(),
        };
        for (rank, (path, title)) in results.into_iter().enumerate() {
            hits.push(Hit {
                book: b.id,
                path,
                title,
                score: 1.0 / (RRF_K + rank as f64 + 1.0),
            });
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.book.cmp(&b.book))
    });
    hits.truncate(50);
    hits
}

fn search_page(shelf: &Shelf, epubs: &[EpubBook], query: &str) -> String {
    let hits = federated_search(shelf, query);
    let mut epub_hits: Vec<(&EpubBook, String, String)> = Vec::new();
    for e in epubs {
        for (path, label) in e.search(query) {
            epub_hits.push((e, path, label));
        }
    }
    let mut body = String::new();
    if hits.is_empty() && epub_hits.is_empty() {
        body = "<p>Nothing aboard matches that.</p>".to_string();
    } else {
        body.push_str("");
        for h in &hits {
            let book_title = shelf
                .book(h.book)
                .map(|b| b.title.as_str())
                .unwrap_or_default();
            body.push_str(&format!(
                "<p class=\"item\">- <a href=\"/book/{}/{}\">{}</a> <span class=\"meta\">in {}</span></p>\n",
                h.book,
                h.path,
                esc(if h.title.is_empty() { &h.path } else { &h.title }),
                esc(book_title)
            ));
        }
        body.push_str("");
        for (e, path, label) in &epub_hits {
            body.push_str(&format!(
                "<p class=\"item\">- <a href=\"/epub/{}/x/{}\">{}</a> <span class=\"meta\">in {} (EPUB)</span></p>\n",
                url_encode(&e.file_name),
                path,
                esc(label),
                esc(&e.title)
            ));
        }
    }
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Search</title></head>\n\
         <body><h1>Search: {}</h1>\n{}\n<p><a href=\"/shelf\">Shelf</a></p></body></html>",
        esc(query),
        body
    )
}

fn books_json(shelf: &Shelf) -> String {
    let mut out = String::from("{\"books\":[");
    for (i, b) in shelf.books.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":{},\"file\":\"{}\",\"title\":\"{}\",\"language\":\"{}\",\"entries\":{},\"articles\":{},\"main_path\":\"{}\"}}",
            b.id,
            json_esc(&b.file_name),
            json_esc(&b.title),
            json_esc(&b.language),
            b.entry_count,
            b.article_count,
            json_esc(&b.main_path)
        ));
    }
    out.push_str("]}");
    out
}

fn search_json(shelf: &Shelf, query: &str) -> String {
    let hits = federated_search(shelf, query);
    let mut out = String::from("{\"results\":[");
    for (i, h) in hits.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let book_title = shelf
            .book(h.book)
            .map(|b| b.title.as_str())
            .unwrap_or_default();
        out.push_str(&format!(
            "{{\"book\":{},\"book_title\":\"{}\",\"path\":\"{}\",\"title\":\"{}\"}}",
            h.book,
            json_esc(book_title),
            json_esc(&h.path),
            json_esc(&h.title)
        ));
    }
    out.push_str("]}");
    out
}

// ---------------------------------------------------------------------
// The catalog: Kiwix's OPDS feed, read when a wire exists. Offline this
// page simply reports that the catalog cannot be reached; the Shelf
// itself never depends on it.

struct CatalogEntry {
    title: String,
    summary: String,
    publisher: String,
    articles: String,
    ftindex: bool,
    size: u64,
    zim_url: String,
    file_name: String,
}

fn xml_tag(block: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = block.find(&open) {
        let rest = &block[start + open.len()..];
        if let Some(end) = rest.find(&close) {
            return rest[..end]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&amp;", "&")
                .trim()
                .to_string();
        }
    }
    String::new()
}

fn xml_attr(tag_text: &str, attr: &str) -> String {
    let pat = format!("{}=\"", attr);
    if let Some(start) = tag_text.find(&pat) {
        let rest = &tag_text[start + pat.len()..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    String::new()
}

fn parse_catalog(xml: &str) -> Vec<CatalogEntry> {
    let mut out = Vec::new();
    for block in xml.split("<entry>").skip(1) {
        let block = block.split("</entry>").next().unwrap_or("");
        let acq = match block.find("acquisition/open-access") {
            Some(i) => i,
            None => continue,
        };
        let tag_end = block[acq..].find("/>").map(|e| acq + e).unwrap_or(block.len());
        let tag_text = &block[acq..tag_end];
        let href = xml_attr(tag_text, "href");
        let zim_url = href.strip_suffix(".meta4").unwrap_or(&href).to_string();
        if !zim_url.ends_with(".zim") {
            continue;
        }
        let file_name = zim_url
            .rsplit('/')
            .next()
            .unwrap_or("book.zim")
            .to_string();
        let publisher = match block.find("<publisher>") {
            Some(start) => {
                let rest = &block[start..];
                let end = rest.find("</publisher>").unwrap_or(rest.len());
                xml_tag(&rest[..end], "name")
            }
            None => String::new(),
        };
        out.push(CatalogEntry {
            title: xml_tag(block, "title"),
            summary: xml_tag(block, "summary"),
            publisher,
            articles: xml_tag(block, "articleCount"),
            ftindex: block.contains("_ftindex:yes"),
            size: xml_attr(tag_text, "length").parse().unwrap_or(0),
            zim_url,
            file_name,
        });
    }
    out
}

fn pretty_category(cat: &str) -> String {
    let spaced = cat.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

fn parse_categories(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for block in xml.split("<entry>").skip(1) {
        let block = block.split("</entry>").next().unwrap_or("");
        let title = xml_tag(block, "title");
        if !title.is_empty() {
            out.push(title);
        }
    }
    out
}

/// The category shelf, cached for ten minutes: it changes rarely, and
/// the catalog page should not pay a second fetch for it every visit.
fn categories(state: &AppState) -> Vec<String> {
    const TTL_SECS: u64 = 600;
    {
        let cache = state.cats.read().unwrap();
        if let Some((when, cats)) = &*cache {
            if when.elapsed().as_secs() < TTL_SECS {
                return cats.clone();
            }
        }
    }
    let fetched = agent_for(state, "https://library.kiwix.org/catalog/v2/categories")
        .get("https://library.kiwix.org/catalog/v2/categories")
        .call()
        .and_then(|r| r.into_string().map_err(ureq::Error::from));
    match fetched {
        Ok(xml) => {
            let cats = parse_categories(&xml);
            *state.cats.write().unwrap() = Some((std::time::Instant::now(), cats.clone()));
            cats
        }
        Err(_) => state
            .cats
            .read()
            .unwrap()
            .as_ref()
            .map(|(_, cats)| cats.clone())
            .unwrap_or_default(),
    }
}

fn catalog_page(state: &AppState, query: &str, lang: &str, cat: &str, src: &str) -> String {
    if src != "kiwix" {
        return feed_catalog_page(state, src, query);
    }
    let lang = if lang.is_empty() { "eng" } else { lang };
    let form = format!(
        "<form action=\"/catalog\" method=\"get\">\
         <input name=\"q\" size=\"32\" value=\"{}\" placeholder=\"Search the catalog\"> \
         <input name=\"lang\" size=\"4\" value=\"{}\"> \
         <input type=\"hidden\" name=\"c\" value=\"{}\"> \
         <input type=\"submit\" value=\"Search\"></form>",
        esc(query),
        esc(lang),
        esc(cat)
    );

    let mut catbar = String::from("<p class=\"cats\">");
    if cat.is_empty() {
        catbar.push_str("<strong>All</strong>");
    } else {
        catbar.push_str(&format!(
            "<a href=\"/catalog?lang={}\">All</a>",
            url_encode(lang)
        ));
    }
    for c in categories(state) {
        catbar.push_str(" | ");
        if c == cat {
            catbar.push_str(&format!("<strong>{}</strong>", esc(&pretty_category(&c))));
        } else {
            catbar.push_str(&format!(
                "<a href=\"/catalog?c={}&lang={}\">{}</a>",
                url_encode(&c),
                url_encode(lang),
                esc(&pretty_category(&c))
            ));
        }
    }
    catbar.push_str("</p>");

    let mut url = "https://library.kiwix.org/catalog/v2/entries?count=24".to_string();
    url.push_str(&format!("&lang={}", url_encode(lang)));
    if !query.is_empty() {
        url.push_str(&format!("&q={}", url_encode(query)));
    }
    if !cat.is_empty() {
        url.push_str(&format!("&category={}", url_encode(cat)));
    }

    let fetched = agent_for(state, &url)
        .get(&url)
        .call()
        .and_then(|r| r.into_string().map_err(ureq::Error::from));
    let body = match fetched {
        Ok(xml) => {
            let entries = parse_catalog(&xml);
            if entries.is_empty() {
                "<p>The catalog has nothing by that name.</p>".to_string()
            } else {
                let shelf = state.shelf();
                let mut groups: std::collections::BTreeMap<String, Vec<&CatalogEntry>> =
                    std::collections::BTreeMap::new();
                for e in &entries {
                    let key = if e.publisher.is_empty() {
                        "Other sources".to_string()
                    } else {
                        e.publisher.clone()
                    };
                    groups.entry(key).or_default().push(e);
                }
                let mut out = String::new();
                for (publisher, es) in &groups {
                    out.push_str(&format!("<h2>{}</h2>\n", esc(publisher)));
                    for e in es {
                        let aboard = shelf.books.iter().any(|b| b.file_name == e.file_name);
                        let meta = format!(
                            "{} articles, {:.1} MB, full text index: {}",
                            esc(&e.articles),
                            e.size as f64 / 1_000_000.0,
                            if e.ftindex { "yes" } else { "no" }
                        );
                        let action = if aboard {
                            "<span class=\"meta\">aboard</span>".to_string()
                        } else {
                            format!(
                                "<a href=\"/acquire?url={}&name={}&size={}\">Add to the Shelf</a>",
                                url_encode(&e.zim_url),
                                url_encode(&e.file_name),
                                e.size
                            )
                        };
                        out.push_str(&format!(
                            "<p class=\"item\">- <strong>{}</strong> - {}<br><span class=\"meta\">{}</span> {}</p>\n",
                            esc(&e.title),
                            esc(&e.summary),
                            meta,
                            action
                        ));
                    }
                    out.push_str("");
                }
                out
            }
        }
        Err(e) => format!(
            "<p>The catalog could not be reached ({}). The Shelf works without it; \
             this page wakes up when a connection does.</p>",
            esc(&e.to_string())
        ),
    };
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Catalog</title></head>\n\
         <body><h1>Catalog</h1>\n{}\n{}\n{}\n{}\n<p><a href=\"/shelf\">Shelf</a></p></body></html>",
        form,
        srcbar("kiwix", lang),
        catbar,
        body
    )
}

// ---------------------------------------------------------------------
// Other catalogs: book feeds beyond Kiwix. Gutenberg, Unglue.it, and
// SermonIndex speak OPDS 1.x (Atom); Open Library speaks OPDS 2.0
// (JSON). Each entry names a file the Shelf can take directly: an EPUB
// or a ZIM. Kiwix stays the stocking catalog; these are side doors.

const SOURCES: [(&str, &str); 5] = [
    ("kiwix", "Kiwix"),
    ("gutenberg", "Gutenberg"),
    ("unglue", "Unglue.it"),
    ("sermon", "SermonIndex"),
    ("openlibrary", "Open Library"),
];

fn srcbar(current: &str, lang: &str) -> String {
    let mut out = String::from("<p class=\"cats\">Sources: ");
    for (i, (key, label)) in SOURCES.iter().enumerate() {
        if i > 0 {
            out.push_str(" | ");
        }
        if *key == current {
            out.push_str(&format!("<strong>{}</strong>", label));
        } else if *key == "kiwix" {
            out.push_str(&format!(
                "<a href=\"/catalog?lang={}\">{}</a>",
                url_encode(lang),
                label
            ));
        } else {
            out.push_str(&format!(
                "<a href=\"/catalog?src={}\">{}</a>",
                key, label
            ));
        }
    }
    out.push_str("</p>");
    out
}

struct FeedEntry {
    title: String,
    author: String,
    summary: String,
    href: String,
    file_name: String,
    kind: String,
}

fn feed_file_name(href: &str, kind: &str, title: &str) -> String {
    let clean = href.split('?').next().unwrap_or(href);
    let clean = clean.strip_suffix(".meta4").unwrap_or(clean);
    let mut name = clean
        .rsplit('/')
        .next()
        .unwrap_or("book")
        .to_string();
    if kind == "epub" {
        if let Some(i) = name.find(".epub") {
            name.truncate(i + 5);
        } else {
            // Some sources serve EPUBs from download endpoints with no
            // file name in the address; name the book from its title.
            let slug: String = title
                .chars()
                .take(40)
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            name = format!("{}-epub.epub", if slug.is_empty() { "book" } else { &slug });
        }
    }
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn abs_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let origin = match base.find("://") {
        Some(i) => {
            let rest = &base[i + 3..];
            let end = rest.find('/').map(|e| i + 3 + e).unwrap_or(base.len());
            &base[..end]
        }
        None => base,
    };
    if let Some(stripped) = href.strip_prefix('/') {
        format!("{}/{}", origin, stripped)
    } else {
        let dir = base.rsplit_once('/').map(|(d, _)| d).unwrap_or(base);
        format!("{}/{}", dir, href)
    }
}

fn parse_feed_opds(xml: &str) -> Vec<FeedEntry> {
    let mut out = Vec::new();
    for block in xml.split("<entry>").skip(1) {
        let block = block.split("</entry>").next().unwrap_or("");
        let title = xml_tag(block, "title");
        if title.is_empty() {
            continue;
        }
        let author = match block.find("<author>") {
            Some(a) => xml_tag(&block[a..], "name"),
            None => String::new(),
        };
        let mut summary = xml_tag(block, "summary");
        if summary.is_empty() {
            summary = xml_tag(block, "content");
        }
        if summary.contains('<') {
            summary = String::new();
        }
        let mut epub_hrefs: Vec<String> = Vec::new();
        let mut zim_href: Option<String> = None;
        let mut page_href: Option<String> = None;
        for (idx, _) in block.match_indices("<link") {
            let rest = &block[idx..];
            let end = rest.find('>').unwrap_or(rest.len());
            let tag = &rest[..end];
            let rel = xml_attr(tag, "rel");
            let typ = xml_attr(tag, "type");
            let href = xml_attr(tag, "href");
            if href.is_empty() {
                continue;
            }
            if rel.contains("subsection") && href.ends_with(".opds") {
                if page_href.is_none() {
                    page_href = Some(href);
                }
                continue;
            }
            if !rel.contains("acquisition") && !rel.is_empty() {
                continue;
            }
            if typ.contains("epub") || href.contains(".epub") {
                epub_hrefs.push(href);
            } else if href.ends_with(".zim") || href.ends_with(".zim.meta4") {
                if zim_href.is_none() {
                    zim_href = Some(href);
                }
            }
        }
        let epub_href = epub_hrefs
            .iter()
            .find(|h| h.contains("noimages"))
            .cloned()
            .or_else(|| epub_hrefs.into_iter().next());
        let (href, kind) = match (epub_href, zim_href) {
            (Some(h), _) => (h, "epub"),
            (None, Some(h)) => (h, "zim"),
            (None, None) => match page_href {
                Some(ph) => (ph, "page"),
                None => continue,
            },
        };
        if kind == "page" {
            out.push(FeedEntry {
                title,
                author,
                summary,
                href,
                file_name: String::new(),
                kind: kind.to_string(),
            });
            continue;
        }
        let href = href
            .strip_suffix(".meta4")
            .map(|h| h.to_string())
            .unwrap_or(href);
        let file_name = feed_file_name(&href, kind, &title);
        if !file_name.ends_with(".epub") && !file_name.ends_with(".zim") {
            continue;
        }
        out.push(FeedEntry {
            title,
            author,
            summary,
            href,
            file_name,
            kind: kind.to_string(),
        });
    }
    out
}

fn parse_opds2(text: &str) -> Vec<FeedEntry> {
    let mut out = Vec::new();
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let mut pubs: Vec<&serde_json::Value> = Vec::new();
    if let Some(arr) = v.get("publications").and_then(|p| p.as_array()) {
        pubs.extend(arr.iter());
    }
    if let Some(groups) = v.get("groups").and_then(|g| g.as_array()) {
        for g in groups {
            if let Some(arr) = g.get("publications").and_then(|p| p.as_array()) {
                pubs.extend(arr.iter());
            }
        }
    }
    for pub_ in pubs {
        let meta = &pub_["metadata"];
        let title = meta
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            continue;
        }
        let author = match meta.get("author") {
            Some(serde_json::Value::String(a)) => a.clone(),
            Some(serde_json::Value::Object(o)) => o
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string(),
            Some(serde_json::Value::Array(a)) => a
                .first()
                .and_then(|f| {
                    f.as_str().map(|s| s.to_string()).or_else(|| {
                        f.get("name").and_then(|n| n.as_str()).map(|s| s.to_string())
                    })
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        let summary = meta
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
        let mut epub_href: Option<String> = None;
        let mut web_href: Option<String> = None;
        if let Some(links) = pub_.get("links").and_then(|l| l.as_array()) {
            for l in links {
                let typ = l.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let rel = l.get("rel").and_then(|t| t.as_str()).unwrap_or("");
                let href = l.get("href").and_then(|h| h.as_str()).unwrap_or("");
                if href.is_empty() {
                    continue;
                }
                if typ.contains("epub") && epub_href.is_none() {
                    epub_href = Some(href.to_string());
                }
                if typ.contains("text/html") && rel.contains("alternate") && web_href.is_none() {
                    web_href = Some(href.to_string());
                }
            }
        }
        if let Some(h) = epub_href {
            let file_name = feed_file_name(&h, "epub", &title);
            if file_name.ends_with(".epub") {
                out.push(FeedEntry {
                    title,
                    author,
                    summary,
                    href: h,
                    file_name,
                    kind: "epub".to_string(),
                });
                continue;
            }
        }
        if let Some(h) = web_href {
            out.push(FeedEntry {
                title,
                author,
                summary,
                href: h,
                file_name: String::new(),
                kind: "web".to_string(),
            });
        }
    }
    out
}

fn feed_catalog_page(state: &AppState, src: &str, query: &str) -> String {
    let (label, url) = match src {
        "gutenberg" => (
            "Project Gutenberg",
            format!(
                "https://www.gutenberg.org/ebooks/search.opds/?query={}",
                url_encode(query)
            ),
        ),
        "unglue" => (
            "Unglue.it",
            "https://unglue.it/api/opds/all/".to_string(),
        ),
        "sermon" => (
            "SermonIndex",
            "https://api.sermonindex.net/kiwix/library.xml".to_string(),
        ),
        _ => (
            "Open Library",
            if query.is_empty() {
                "https://openlibrary.org/opds/".to_string()
            } else {
                format!(
                    "https://openlibrary.org/opds/search?query={}",
                    url_encode(query)
                )
            },
        ),
    };
    let form = format!(
        "<form action=\"/catalog\" method=\"get\">         <input type=\"hidden\" name=\"src\" value=\"{}\">          <input name=\"q\" size=\"32\" value=\"{}\" placeholder=\"Search {}\">          <input type=\"submit\" value=\"Search\"></form>",
        esc(src),
        esc(query),
        label
    );
    let fetched = agent_for(state, &url)
        .get(&url)
        .call()
        .and_then(|r| r.into_string().map_err(ureq::Error::from));
    let body = match fetched {
        Ok(text) => {
            let mut entries = if src == "openlibrary" {
                parse_opds2(&text)
            } else {
                parse_feed_opds(&text)
            };
            if src == "gutenberg" {
                // Gutenberg's search feed names book pages, not files:
                // follow each book page and take the file it offers.
                let mut resolved: Vec<FeedEntry> = Vec::new();
                let mut followed = 0;
                for e in entries.into_iter() {
                    if e.kind != "page" {
                        resolved.push(e);
                        continue;
                    }
                    let is_book_page = e
                        .href
                        .rsplit_once("/ebooks/")
                        .map(|(_, tail)| {
                            tail.ends_with(".opds")
                                && tail[..tail.len() - 5]
                                    .chars()
                                    .all(|c| c.is_ascii_digit())
                        })
                        .unwrap_or(false);
                    if !is_book_page || followed >= 8 {
                        continue;
                    }
                    followed += 1;
                    let page_url = abs_url("https://www.gutenberg.org/", &e.href);
                    if let Ok(page) = agent_for(state, &page_url)
                        .get(&page_url)
                        .call()
                        .and_then(|r| r.into_string().map_err(ureq::Error::from))
                    {
                        if let Some(mut sub) = parse_feed_opds(&page)
                            .into_iter()
                            .find(|x| x.kind == "epub" || x.kind == "zim")
                        {
                            if sub.title.is_empty() {
                                sub.title = e.title.clone();
                            }
                            if sub.author.is_empty() {
                                sub.author = e.author.clone();
                            }
                            sub.href = abs_url("https://www.gutenberg.org/", &sub.href);
                            sub.file_name =
                                feed_file_name(&sub.href, &sub.kind, &sub.title);
                            resolved.push(sub);
                        }
                    }
                }
                entries = resolved;
            }
            if (src == "sermon" || src == "unglue") && !query.is_empty() {
                let q = query.to_lowercase();
                entries.retain(|e| e.title.to_lowercase().contains(&q));
            }
            if entries.is_empty() {
                "<p>That catalog has nothing by that name.</p>".to_string()
            } else {
                let shelf = state.shelf();
                let epubs = state.epubs();
                let mut out = String::new();
                for e in &entries {
                    if e.kind == "web" {
                        out.push_str(&format!(
                            "<p class=\"item\">- <strong>{}</strong> - {}<br><span class=\"meta\">Open Library lends this book</span> <a href=\"{}\">Borrow there</a></p>\n",
                            esc(&e.title),
                            esc(&e.summary),
                            esc(&e.href)
                        ));
                        continue;
                    }
                    let aboard = shelf.books.iter().any(|b| b.file_name == e.file_name)
                        || epubs.iter().any(|b| b.file_name == e.file_name);
                    let meta = if e.author.is_empty() {
                        format!("{} book", e.kind.to_uppercase())
                    } else {
                        format!("{} book, by {}", e.kind.to_uppercase(), esc(&e.author))
                    };
                    let action = if aboard {
                        "<span class=\"meta\">aboard</span>".to_string()
                    } else {
                        format!(
                            "<a href=\"/acquire?url={}&name={}&size=0\">Add to the Shelf</a>",
                            url_encode(&e.href),
                            url_encode(&e.file_name)
                        )
                    };
                    out.push_str(&format!(
                        "<p class=\"item\">- <strong>{}</strong> - {}<br><span class=\"meta\">{}</span> {}</p>\n",
                        esc(&e.title),
                        esc(&e.summary),
                        meta,
                        action
                    ));
                }
                out
            }
        }
        Err(e) => format!(
            "<p>{} could not be reached ({}). The Shelf works without it;              this page wakes up when a connection does.</p>",
            label,
            esc(&e.to_string())
        ),
    };
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Catalog</title></head>\n\
         <body><h1>Catalog</h1>\n{}\n{}\n{}\n<p><a href=\"/shelf\">Shelf</a></p></body></html>",
        form,
        srcbar(src, "eng"),
        body
    )
}

// ---------------------------------------------------------------------
// Fetching books: each download runs on its own thread and reports
// its state here, so the progress page can watch them all come down.
// Several fetch at once; the Shelf keeps serving throughout. A fetch
// that stops leaves its partial file in the shelf directory, and
// fetching the same book again resumes from those bytes when the
// source honors range requests - a connection that comes and goes
// still finishes the book, a few sips at a time.

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Running,
    Done,
    Failed,
}

struct Download {
    id: usize,
    url: String,
    name: String,
    received: u64,
    total: u64,
    phase: Phase,
    book: usize,
    title: String,
    error: String,
    link: String,
}

fn set_dl(state: &AppState, id: usize, f: impl FnOnce(&mut Download)) {
    let mut all = state.downloads.write().unwrap();
    if let Some(d) = all.iter_mut().find(|d| d.id == id) {
        f(d);
    }
}

fn dl_failed(state: &AppState, id: usize, msg: String) {
    set_dl(state, id, |d| {
        d.phase = Phase::Failed;
        d.error = msg;
    });
}

fn download_item_json(d: &Download) -> String {
    let phase = match d.phase {
        Phase::Idle => "idle",
        Phase::Running => "running",
        Phase::Done => "done",
        Phase::Failed => "failed",
    };
    let percent = if d.total > 0 {
        d.received * 100 / d.total
    } else {
        0
    };
    format!(
        "{{\"id\":{},\"phase\":\"{}\",\"name\":\"{}\",\"received\":{},\"total\":{},\"percent\":{},\"book\":{},\"title\":\"{}\",\"error\":\"{}\",\"link\":\"{}\"}}",
        d.id,
        phase,
        json_esc(&d.name),
        d.received,
        d.total,
        percent,
        d.book,
        json_esc(&d.title),
        json_esc(&d.error),
        json_esc(&d.link)
    )
}

fn downloads_json(state: &AppState) -> String {
    let all = state.downloads.read().unwrap();
    let items: Vec<String> = all.iter().rev().map(download_item_json).collect();
    format!("{{\"downloads\":[{}]}}", items.join(","))
}

const PROGRESS_PAGE: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Fetching</title>
<style>
body{font-family:system-ui,sans-serif;margin:24px;color:#111}
.track{height:22px;background:#eee;border:1px solid #bbb;max-width:640px}
.bar{height:100%;width:0;background:#3b6fd4;transition:width .4s}
.dl{margin-bottom:18px}
</style></head>
<body>
<h1>Fetching</h1>
<div id="list"></div>
<p><a href="/shelf">Shelf</a> - reading keeps working while books come down.</p>
<script>
function mb(n){ return (n/1000000).toFixed(1) + ' MB'; }
function esc(s){ var d=document.createElement('div'); d.textContent=s; return d.innerHTML; }
function tick(){
  fetch('/api/acquire-status').then(function(r){ return r.json(); }).then(function(j){
    var list = document.getElementById('list');
    list.innerHTML = '';
    var anyRunning = false;
    if(!j.downloads.length){ list.innerHTML = '<p>nothing is fetching right now.</p>'; }
    j.downloads.forEach(function(d){
      var div = document.createElement('div'); div.className = 'dl';
      var html = '<p><strong>' + esc(d.name) + '</strong></p>';
      if(d.phase === 'running'){
        anyRunning = true;
        html += '<div class="track"><div class="bar" style="width:' + (d.total > 0 ? d.percent : 0) + '%"></div></div>';
        html += '<p>' + (d.total > 0 ? mb(d.received) + ' of ' + mb(d.total) + ' - ' + d.percent + '%' : mb(d.received) + ' so far') + '</p>';
      } else if(d.phase === 'done'){
        html += '<div class="track"><div class="bar" style="width:100%"></div></div>';
        html += '<p>down: ' + mb(d.received) + '. <a href="' + (d.link ? d.link : '/book/' + d.book) + '">' + esc(d.title) + '</a> is aboard.</p>';
      } else if(d.phase === 'failed'){
        html += '<p>the fetch did not finish: ' + esc(d.error) + '</p>';
      }
      div.innerHTML = html;
      list.appendChild(div);
    });
    if(anyRunning){ setTimeout(tick, 600); }
  }).catch(function(){ setTimeout(tick, 1500); });
}
tick();
</script>
</body></html>"##;

fn acquire(state: Arc<AppState>, url: &str, name: &str, size: u64) -> (u16, String) {
    let name_ok = !name.is_empty()
        && (name.ends_with(".zim") || name.ends_with(".epub"))
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !name_ok {
        return (400, "<p>That file name is not one the Shelf can take.</p>".to_string());
    }
    let url_ok = url.starts_with("https://")
        && ((name.ends_with(".zim") && url.ends_with(".zim"))
            || (name.ends_with(".epub")
                && (url.contains(".epub") || url.starts_with("https://unglue.it/"))));
    if !url_ok {
        return (400, "<p>That is not a book address.</p>".to_string());
    }

    let part_len = std::fs::metadata(state.dir.join(format!("{}.part", name)))
        .map(|m| m.len())
        .unwrap_or(0);
    let id = {
        let mut all = state.downloads.write().unwrap();
        if let Some(d) = all
            .iter()
            .find(|d| d.phase == Phase::Running && d.name == name)
        {
            return (
                409,
                format!(
                    "<p>Already fetching {} - <a href=\"/acquire-progress\">watch it come down</a>.</p>",
                    esc(&d.name)
                ),
            );
        }
        let id = all.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        all.push(Download {
            id,
            url: url.to_string(),
            name: name.to_string(),
            received: part_len,
            total: size,
            phase: Phase::Running,
            book: 0,
            title: String::new(),
            error: String::new(),
            link: String::new(),
        });
        id
    };

    let state2 = state.clone();
    std::thread::spawn(move || run_download(state2, id));
    (200, PROGRESS_PAGE.to_string())
}

fn run_download(state: Arc<AppState>, id: usize) {
    use std::io::Write;

    let (url, name) = {
        let all = state.downloads.read().unwrap();
        match all.iter().find(|d| d.id == id) {
            Some(d) => (d.url.clone(), d.name.clone()),
            None => return,
        }
    };
    let part = state.dir.join(format!("{}.part", name));
    let dest = state.dir.join(&name);
    let mut have: u64 = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    let mut req = agent_for(&state, &url).get(&url);
    if have > 0 {
        req = req.set("Range", &format!("bytes={}-", have));
    }
    let response = match req.call() {
        Ok(r) => r,
        Err(e) => {
            dl_failed(&state, id, format!("fetch failed: {} - fetch again to resume", e));
            return;
        }
    };
    let status = response.status();
    let resuming = have > 0 && status == 206;
    if have > 0 && !resuming && status != 200 {
        dl_failed(
            &state,
            id,
            format!("the source answered {} - fetch again to resume", status),
        );
        return;
    }
    if !resuming {
        have = 0;
    }
    if let Some(len) = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
    {
        if len > 0 {
            let total = have + len;
            set_dl(&state, id, |d| d.total = total);
        }
    }

    let opened = if resuming {
        std::fs::OpenOptions::new().append(true).open(&part)
    } else {
        std::fs::File::create(&part)
    };
    let mut file = match opened {
        Ok(f) => f,
        Err(e) => {
            dl_failed(&state, id, e.to_string());
            return;
        }
    };
    let mut reader = response.into_reader();
    let mut buf = [0u8; 65536];
    let mut received: u64 = have;
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = file.write_all(&buf[..n]) {
                    dl_failed(&state, id, format!("{} - fetch again to resume", e));
                    return;
                }
                received += n as u64;
                let r = received;
                set_dl(&state, id, |d| d.received = r);
            }
            Err(e) => {
                dl_failed(&state, id, format!("{} - fetch again to resume", e));
                return;
            }
        }
    }
    drop(file);

    let magic_ok = std::fs::File::open(&part)
        .and_then(|mut f| {
            let mut head = [0u8; 3];
            f.read_exact(&mut head).map(|_| head)
        })
        .map(|head| {
            if name.ends_with(".epub") {
                &head[..2] == b"PK"
            } else {
                &head == b"ZIM"
            }
        })
        .unwrap_or(false);
    if !magic_ok {
        let _ = std::fs::remove_file(&part);
        dl_failed(&state, id, "the download is not the book it claimed to be".to_string());
        return;
    }
    if let Err(e) = std::fs::rename(&part, &dest) {
        dl_failed(&state, id, e.to_string());
        return;
    }

    state.reload();
    if name.ends_with(".epub") {
        let epubs = state.epubs();
        match epubs.iter().find(|b| b.file_name == name) {
            Some(book) => {
                let title = book.title.clone();
                let link = format!("/epub/{}/", url_encode(&book.file_name));
                set_dl(&state, id, |d| {
                    d.phase = Phase::Done;
                    d.title = title;
                    d.link = link;
                });
            }
            None => dl_failed(&state, id, "downloaded, but the book would not open".to_string()),
        }
    } else {
        let shelf = state.shelf();
        match shelf.books.iter().find(|b| b.file_name == name) {
            Some(book) => {
                let bid = book.id;
                let title = book.title.clone();
                let link = format!("/book/{}", bid);
                set_dl(&state, id, |d| {
                    d.phase = Phase::Done;
                    d.book = bid;
                    d.title = title;
                    d.link = link;
                });
            }
            None => dl_failed(&state, id, "downloaded, but the book would not open".to_string()),
        }
    }
}

// ---------------------------------------------------------------------

// Bookmarks and history: the reader's own marks, kept in a plain
// JSON file in the shelf directory - readable, movable, theirs.

#[derive(Clone)]
struct Mark {
    url: String,
    title: String,
}

#[derive(Default)]
struct BrowserData {
    bookmarks: Vec<Mark>,
    history: Vec<Mark>,
}

fn load_data(dir: &std::path::Path) -> BrowserData {
    let mut data = BrowserData::default();
    // Shelves kept under the previous name carry umiak-data.json;
    // read it until the first save writes coffer-data.json.
    let text = std::fs::read_to_string(dir.join("coffer-data.json"))
        .or_else(|_| std::fs::read_to_string(dir.join("umiak-data.json")));
    if let Ok(text) = text {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            for key in ["bookmarks", "history"] {
                if let Some(arr) = v.get(key).and_then(|a| a.as_array()) {
                    let list: Vec<Mark> = arr
                        .iter()
                        .filter_map(|m| {
                            let url = m.get("url")?.as_str()?.to_string();
                            let title = m
                                .get("title")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(Mark { url, title })
                        })
                        .collect();
                    if key == "bookmarks" {
                        data.bookmarks = list;
                    } else {
                        data.history = list;
                    }
                }
            }
        }
    }
    data
}

fn record_history(state: &AppState, url: &str, title: &str) {
    if url.is_empty() || url.starts_with("/api/") {
        return;
    }
    {
        let mut data = state.data.write().unwrap();
        if data.history.last().map(|m| m.url == url).unwrap_or(false) {
            return;
        }
        data.history.push(Mark {
            url: url.to_string(),
            title: title.to_string(),
        });
        if data.history.len() > 300 {
            let excess = data.history.len() - 300;
            data.history.drain(..excess);
        }
    }
    state.save_data();
}

fn toggle_bookmark(state: &AppState, url: &str, title: &str) -> bool {
    let now;
    {
        let mut data = state.data.write().unwrap();
        if let Some(i) = data.bookmarks.iter().position(|m| m.url == url) {
            data.bookmarks.remove(i);
            now = false;
        } else {
            data.bookmarks.push(Mark {
                url: url.to_string(),
                title: title.to_string(),
            });
            now = true;
        }
    }
    state.save_data();
    now
}

fn bookmarks_page(state: &AppState) -> String {
    let data = state.data.read().unwrap();
    let mut body = String::new();
    if data.bookmarks.is_empty() {
        body = "<p>No bookmarks yet. The star in the bar keeps the page you are reading.</p>".to_string();
    } else {
        for m in &data.bookmarks {
            body.push_str(&format!(
                "<p class=\"item\">- <a href=\"{}\">{}</a> <span class=\"meta\">(<a href=\"/bookmark-remove?url={}\">remove</a>)</span></p>\n",
                esc(&m.url),
                esc(if m.title.is_empty() { &m.url } else { &m.title }),
                url_encode(&m.url)
            ));
        }
    }
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Bookmarks</title></head>\n\
         <body><h1>Bookmarks</h1>\n{}\n<p><a href=\"/shelf\">Shelf</a></p></body></html>",
        body
    )
}

fn history_page(state: &AppState) -> String {
    let data = state.data.read().unwrap();
    let mut body = String::new();
    if data.history.is_empty() {
        body = "<p>Nothing read yet in this browser.</p>".to_string();
    } else {
        for m in data.history.iter().rev() {
            body.push_str(&format!(
                "<p class=\"item\">- <a href=\"{}\">{}</a></p>\n",
                esc(&m.url),
                esc(if m.title.is_empty() { &m.url } else { &m.title })
            ));
        }
    }
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>History</title></head>\n\
         <body><h1>History</h1>\n{}\n<p><a href=\"/shelf\">Shelf</a></p></body></html>",
        body
    )
}

struct AppState {
    dir: PathBuf,
    data: RwLock<BrowserData>,
    shelf: RwLock<Arc<Shelf>>,
    epubs: RwLock<Arc<Vec<EpubBook>>>,
    maps: RwLock<Arc<Vec<MapItem>>>,
    sync_job: RwLock<Option<SyncJob>>,
    downloads: RwLock<Vec<Download>>,
    cats: RwLock<Option<(std::time::Instant, Vec<String>)>>,
    agent: ureq::Agent,
    direct_agent: ureq::Agent,
}

/// The agent for a URL: loopback and private addresses, and hosts the
/// environment's NO_PROXY names, are fetched directly - a proxy is a
/// supply line to the outside, never a tollbooth on the reader's own
/// street.
fn agent_for<'a>(state: &'a AppState, url: &str) -> &'a ureq::Agent {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .last()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let private = host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.starts_with("192.168.")
        || host.starts_with("10.")
        || (host.starts_with("172.")
            && host
                .split('.')
                .nth(1)
                .and_then(|o| o.parse::<u8>().ok())
                .map(|o| (16..=31).contains(&o))
                .unwrap_or(false));
    if private {
        return &state.direct_agent;
    }
    for var in ["NO_PROXY", "no_proxy"] {
        if let Ok(list) = std::env::var(var) {
            for entry in list.split(',') {
                let entry = entry.trim().to_lowercase();
                if entry.is_empty() {
                    continue;
                }
                if entry == "*" || host == entry || host.ends_with(&format!(".{}", entry.trim_start_matches('.'))) {
                    return &state.direct_agent;
                }
            }
        }
    }
    &state.agent
}

impl AppState {
    fn shelf(&self) -> Arc<Shelf> {
        self.shelf.read().unwrap().clone()
    }

    fn epubs(&self) -> Arc<Vec<EpubBook>> {
        self.epubs.read().unwrap().clone()
    }

    fn save_data(&self) {
        let data = self.data.read().unwrap();
        fn marks_json(marks: &[Mark]) -> String {
            marks
                .iter()
                .map(|m| {
                    format!(
                        "{{\"url\":\"{}\",\"title\":\"{}\"}}",
                        json_esc(&m.url),
                        json_esc(&m.title)
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        }
        let text = format!(
            "{{\"bookmarks\":[{}],\"history\":[{}]}}\n",
            marks_json(&data.bookmarks),
            marks_json(&data.history)
        );
        let _ = std::fs::write(self.dir.join("coffer-data.json"), text);
    }

    fn maps(&self) -> Arc<Vec<MapItem>> {
        self.maps.read().unwrap().clone()
    }

    fn reload(&self) {
        let fresh = Arc::new(Shelf::open_dir(&self.dir));
        *self.shelf.write().unwrap() = fresh;
        let fresh_epubs = Arc::new(epub::scan_epubs(&self.dir));
        *self.epubs.write().unwrap() = fresh_epubs;
        let fresh_maps = Arc::new(scan_maps(&self.dir));
        *self.maps.write().unwrap() = fresh_maps;
    }
}


pub struct MapItem {
    file_name: String,
    archive: pmtiles::Archive,
}

pub fn scan_maps(dir: &PathBuf) -> Vec<MapItem> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("pmtiles"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    for path in paths {
        match pmtiles::Archive::open(&path) {
            Ok(archive) => out.push(MapItem {
                file_name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                archive,
            }),
            Err(e) => eprintln!("coffer-server: skipping {}: {e}", path.display()),
        }
    }
    out
}

fn maps_page(maps: &[MapItem]) -> String {
    let mut body = String::from(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Maps</title></head>\n<body><h1>Maps</h1>\n",
    );
    if maps.is_empty() {
        body.push_str("<p>No map archives aboard yet. A map archive is a single .pmtiles file - Protomaps publishes them, from a city extract to the whole planet - and it goes in the shelf directory beside the books.</p>\n");
    }
    for m in maps {
        let h = &m.archive.header;
        body.push_str(&format!(
            "<p class=\"item\">- <a href=\"/map/{}/\">{}</a> <span class=\"meta\">{} - zooms {} to {} - {} tile entries - bounds {:.2}, {:.2} to {:.2}, {:.2}</span></p>\n",
            url_encode(&m.file_name),
            esc(&m.file_name),
            h.tile_type.label(),
            h.min_zoom,
            h.max_zoom,
            h.num_entries,
            h.min_lon,
            h.min_lat,
            h.max_lon,
            h.max_lat
        ));
    }
    body.push_str("<p><a href=\"/shelf\">Shelf</a></p></body></html>");
    body
}

fn lon_to_tilex(lon: f64, z: u8) -> f64 {
    (lon + 180.0) / 360.0 * (1u64 << z) as f64
}

fn lat_to_tiley(lat: f64, z: u8) -> f64 {
    let lat = lat.clamp(-85.0511, 85.0511).to_radians();
    (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * (1u64 << z) as f64
}

fn tilex_to_lon(x: f64, z: u8) -> f64 {
    x / (1u64 << z) as f64 * 360.0 - 180.0
}

fn tiley_to_lat(y: f64, z: u8) -> f64 {
    let n = std::f64::consts::PI * (1.0 - 2.0 * y / (1u64 << z) as f64);
    (n.sinh().atan()).to_degrees()
}

fn map_viewer_page(item: &MapItem, z: u8, lon: f64, lat: f64) -> String {
    let h = &item.archive.header;
    let file = url_encode(&item.file_name);
    let mut body = format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{}</title></head>\n<body><h1>{}</h1>\n",
        esc(&item.file_name),
        esc(&item.file_name)
    );
    if h.tile_type != pmtiles::TileType::Png
        && h.tile_type != pmtiles::TileType::Jpg
        && h.tile_type != pmtiles::TileType::Webp
    {
        body.push_str(&format!(
            "<p>This archive holds {} tiles. Tiles are served at /map/{}/tile/Z/X/Y for any reader that speaks vector tiles; the picture viewer in this build renders raster archives.</p>\n",
            h.tile_type.label(),
            file
        ));
        body.push_str("<p><a href=\"/maps\">Maps</a></p></body></html>");
        return body;
    }
    let cx = lon_to_tilex(lon, z);
    let cy = lat_to_tiley(lat, z);
    let (cxi, cyi) = (cx.floor() as i64, cy.floor() as i64);
    let n = 1i64 << z;
    let link = |z: u8, lon: f64, lat: f64| {
        format!("/map/{}/?z={}&lon={:.5}&lat={:.5}", file, z, lon, lat)
    };
    body.push_str("<p>");
    if z < h.max_zoom {
        body.push_str(&format!("<a href=\"{}\">Zoom in</a> ", link(z + 1, lon, lat)));
    }
    if z > h.min_zoom {
        body.push_str(&format!("<a href=\"{}\">Zoom out</a> ", link(z - 1, lon, lat)));
    }
    let step_lon = tilex_to_lon(cx + 1.0, z) - tilex_to_lon(cx, z);
    let step_lat = tiley_to_lat(cy, z) - tiley_to_lat(cy + 1.0, z);
    body.push_str(&format!(
        "<a href=\"{}\">North</a> <a href=\"{}\">South</a> <a href=\"{}\">West</a> <a href=\"{}\">East</a></p>\n",
        link(z, lon, lat + step_lat),
        link(z, lon, lat - step_lat),
        link(z, lon - step_lon, lat),
        link(z, lon + step_lon, lat)
    ));
    body.push_str("<table cellspacing=\"0\" cellpadding=\"0\">");
    for dy in -1i64..=1 {
        body.push_str("<tr>");
        for dx in -1i64..=1 {
            let (tx, ty) = (cxi + dx, cyi + dy);
            if tx < 0 || ty < 0 || tx >= n || ty >= n {
                body.push_str("<td width=\"256\" height=\"256\"></td>");
            } else {
                body.push_str(&format!(
                    "<td><img width=\"256\" height=\"256\" src=\"/map/{}/tile/{}/{}/{}\"></td>",
                    file, z, tx, ty
                ));
            }
        }
        body.push_str("</tr>");
    }
    body.push_str("</table>\n");
    if let Some(meta) = item.archive.metadata() {
        if let Some(att) = meta.get("attribution").and_then(|a| a.as_str()) {
            body.push_str(&format!("<p>{}</p>\n", att));
        }
    }
    body.push_str("<p><a href=\"/maps\">Maps</a></p></body></html>");
    body
}

fn map_route(state: &AppState, rest: &str, url: &str) -> Response<Cursor<Vec<u8>>> {
    let (file_part, inner) = match rest.split_once('/') {
        Some((a, b)) => (a, b.to_string()),
        None => (rest, String::new()),
    };
    let file_name = url_decode(file_part);
    let maps = state.maps();
    let item = match maps.iter().find(|m| m.file_name == file_name) {
        Some(m) => m,
        None => return Response::from_string("no such map archive\n").with_status_code(404),
    };
    if let Some(tile_path) = inner.strip_prefix("tile/") {
        let parts: Vec<&str> = tile_path.split('/').collect();
        if parts.len() == 3 {
            let parsed = (
                parts[0].parse::<u8>(),
                parts[1].parse::<u64>(),
                parts[2].parse::<u64>(),
            );
            if let (Ok(z), Ok(x), Ok(y)) = parsed {
                return match item.archive.tile(z, x, y) {
                    Ok(Some(bytes)) => {
                        let header = tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            item.archive.header.tile_type.content_type().as_bytes(),
                        )
                        .unwrap();
                        Response::from_data(bytes).with_header(header)
                    }
                    Ok(None) => {
                        Response::from_string("no tile there\n").with_status_code(404)
                    }
                    Err(e) => Response::from_string(format!("tile failed: {e}\n"))
                        .with_status_code(500),
                };
            }
        }
        return Response::from_string("bad tile path\n").with_status_code(400);
    }
    let h = &item.archive.header;
    let z = query_param(url, "z")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(h.center_zoom)
        .clamp(h.min_zoom, h.max_zoom);
    let lon = query_param(url, "lon")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(h.center_lon);
    let lat = query_param(url, "lat")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(h.center_lat);
    html_response(map_viewer_page(item, z, lon, lat))
}

pub struct SyncJob {
    source: String,
    status: String,
    chunks_done: usize,
    chunks_total: usize,
    finished: bool,
    failed: bool,
}

fn trusted_keys(dir: &PathBuf) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join("trusted-keys.txt")) else {
        return Vec::new();
    };
    text.lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn fetch_source_bytes(state: &AppState, source: &str, tail: &str) -> Result<Vec<u8>, String> {
    if let Some(dir) = source.strip_prefix("file://") {
        return std::fs::read(format!("{dir}/{tail}"))
            .map_err(|e| format!("cannot read {dir}/{tail}: {e}"));
    }
    if source.starts_with('/') {
        return std::fs::read(format!("{source}/{tail}"))
            .map_err(|e| format!("cannot read {source}/{tail}: {e}"));
    }
    let url = format!("{}/{tail}", source.trim_end_matches('/'));
    let agent = agent_for(state, &url);
    let resp = agent
        .get(&url)
        .call()
        .map_err(|e| format!("fetch {url} failed: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take((sync::CHUNK_MAX + 1024 * 1024) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| format!("read {url} failed: {e}"))?;
    Ok(buf)
}

fn set_job(state: &AppState, update: impl FnOnce(&mut SyncJob)) {
    if let Some(job) = state.sync_job.write().unwrap().as_mut() {
        update(job);
    }
}

fn sync_run(state: Arc<AppState>, source: String, allow_unsigned: bool) {
    fn fail_job(state: &AppState, why: String) {
        set_job(state, |j| {
            j.status = why;
            j.finished = true;
            j.failed = true;
        });
    }
    let manifest_raw = match fetch_source_bytes(&state, &source, "edition.json") {
        Ok(b) => b,
        Err(e) => return fail_job(&state, e),
    };
    let edition: sync::Edition = match serde_json::from_slice(&manifest_raw) {
        Ok(e) => e,
        Err(e) => return fail_job(&state, format!("edition manifest is unreadable: {e}")),
    };
    if edition.format != "coffer-edition/0" && edition.format != "umiak-edition/0" {
        return fail_job(&state, format!("unknown edition format {}", edition.format));
    }
    match (&edition.signer, &edition.signature) {
        (Some(_), Some(_)) => {
            let keys = trusted_keys(&state.dir);
            if let Err(e) = sync::verify_edition(&edition, &keys) {
                return fail_job(&state, e);
            }
            set_job(&state, |j| j.status = "signature verified".to_string());
        }
        _ => {
            if !allow_unsigned {
                return fail_job(
                    &state,
                    "edition is unsigned - refusing without the owner's say-so".to_string(),
                );
            }
            set_job(&state, |j| {
                j.status = "warning: unsigned edition, accepted on the owner's word".to_string()
            });
        }
    }
    let total: usize = edition.files.iter().map(|f| f.chunks.len()).sum();
    set_job(&state, |j| {
        j.chunks_total = total;
        j.status = format!("fetching chunks for {}", edition.name);
    });
    let pool = sync::pool_dir(&state.dir);
    if let Err(e) = std::fs::create_dir_all(&pool) {
        return fail_job(&state, format!("cannot open the chunk pool: {e}"));
    }
    for file in &edition.files {
        let target = state.dir.join(&file.name);
        if target.exists() {
            if let Ok(existing) = sync::read_file(&target) {
                if sync::sha256_hex(&existing) == file.sha256 {
                    set_job(&state, |j| j.chunks_done += file.chunks.len());
                    continue;
                }
            }
        }
        for chunk in &file.chunks {
            if !sync::pool_has(&pool, chunk) {
                let bytes = match fetch_source_bytes(&state, &source, &format!("chunks/{chunk}")) {
                    Ok(b) => b,
                    Err(e) => return fail_job(&state, e),
                };
                if let Err(e) = sync::pool_put(&pool, chunk, &bytes) {
                    return fail_job(&state, e);
                }
            }
            set_job(&state, |j| j.chunks_done += 1);
        }
        match sync::assemble(&pool, file) {
            Ok(bytes) => {
                let part = state.dir.join(format!("{}.part", file.name));
                if let Err(e) = std::fs::write(&part, &bytes)
                    .and_then(|()| std::fs::rename(&part, &target))
                {
                    return fail_job(&state, format!("cannot shelve {}: {e}", file.name));
                }
            }
            Err(e) => return fail_job(&state, e),
        }
    }
    state.reload();
    set_job(&state, |j| {
        j.status = format!("edition {} shelved", edition.name);
        j.finished = true;
    });
}

fn sync_page(state: &AppState) -> String {
    let keys = trusted_keys(&state.dir);
    let job = state.sync_job.read().unwrap();
    let mut body = String::from(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Sync</title>",
    );
    if let Some(j) = job.as_ref() {
        if !j.finished {
            body.push_str("<meta http-equiv=\"refresh\" content=\"3\">");
        }
    }
    body.push_str("</head>\n<body><h1>Sync an edition</h1>\n");
    body.push_str("<p>An edition is a signed manifest plus content-defined chunks, held by a mirror - an http address, or a directory on a disk that arrived by hand. Sync fetches only the chunks this shelf does not already have, verifies every chunk and every assembled file by hash, and shelves what passes.</p>\n");
    if keys.is_empty() {
        body.push_str("<p>No trusted keys yet: signed editions will be refused until a public key (one hex key per line) is placed in trusted-keys.txt in the shelf directory. Unsigned editions need the box below, ticked on purpose.</p>\n");
    } else {
        body.push_str(&format!("<p>{} trusted key(s) named in trusted-keys.txt.</p>\n", keys.len()));
    }
    body.push_str("<form action=\"/api/sync-start\" method=\"get\"><input name=\"source\" size=\"60\" placeholder=\"Mirror address or directory\"> <label><input type=\"checkbox\" name=\"allow_unsigned\" value=\"1\"> allow unsigned</label> <input type=\"submit\" value=\"Sync\"></form>\n");
    if let Some(j) = job.as_ref() {
        body.push_str(&format!(
            "<p>- {} - {} of {} chunks - from {}</p>\n",
            esc(&j.status), j.chunks_done, j.chunks_total, esc(&j.source)
        ));
    }
    body.push_str("<p><a href=\"/shelf\">Shelf</a></p></body></html>");
    body
}

fn epub_route(state: &AppState, rest: &str) -> Response<Cursor<Vec<u8>>> {
    let (file_part, inner) = match rest.split_once('/') {
        Some((a, b)) => (a, b.to_string()),
        None => (rest, String::new()),
    };
    let file_name = url_decode(file_part);
    let epubs = state.epubs();
    let book = match epubs.iter().find(|e| e.file_name == file_name) {
        Some(b) => b,
        None => return Response::from_string("no such book\n").with_status_code(404),
    };
    if inner.is_empty() {
        let mut body = format!(
            "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{}</title></head>\n<body><h1>{}</h1>\n",
            esc(&book.title),
            esc(&book.title)
        );
        for (path, label) in &book.chapters {
            body.push_str(&format!(
                "<p class=\"item\">- <a href=\"/epub/{}/x/{}\">{}</a></p>\n",
                url_encode(&book.file_name),
                path,
                esc(label)
            ));
        }
        body.push_str("<p><a href=\"/shelf\">Shelf</a></p></body></html>");
        return html_response(body);
    }
    if let Some(inner_path) = inner.strip_prefix("x/") {
        let decoded = url_decode(inner_path);
        return match book.entry(&decoded) {
            Some((mime, bytes)) => Response::from_data(bytes).with_header(
                Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap(),
            ),
            None => Response::from_string("not in this book\n").with_status_code(404),
        };
    }
    Response::from_string("not found\n").with_status_code(404)
}

fn handle(request: tiny_http::Request, state: Arc<AppState>) {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let shelf = state.shelf();

    let response = if path == "/" {
        html_response(CHROME.to_string())
    } else if path == "/shelf" {
        html_response(shelf_page(&shelf, &state.epubs()))
    } else if path == "/sync" {
        html_response(sync_page(&state))
    } else if path == "/api/sync-start" {
        let source = query_param(&url, "source").unwrap_or_default();
        let allow_unsigned = query_param(&url, "allow_unsigned").is_some();
        if source.is_empty() {
            Response::from_string("a sync needs a source\n").with_status_code(400)
        } else if state.sync_job.read().unwrap().as_ref().map(|j| !j.finished).unwrap_or(false) {
            Response::from_string("a sync is already running\n").with_status_code(409)
        } else {
            *state.sync_job.write().unwrap() = Some(SyncJob {
                source: source.clone(),
                status: "starting".to_string(),
                chunks_done: 0,
                chunks_total: 0,
                finished: false,
                failed: false,
            });
            let st = state.clone();
            std::thread::spawn(move || sync_run(st, source, allow_unsigned));
            let header = tiny_http::Header::from_bytes(&b"Location"[..], &b"/sync"[..]).unwrap();
            Response::from_string("").with_status_code(302).with_header(header)
        }
    } else if path == "/api/sync-status" {
        let job = state.sync_job.read().unwrap();
        let text = match job.as_ref() {
            Some(j) => serde_json::json!({
                "status": j.status,
                "done": j.chunks_done,
                "total": j.chunks_total,
                "finished": j.finished,
                "failed": j.failed
            }).to_string() + "\n",
            None => "{\"status\": \"no sync has run\", \"done\": 0, \"total\": 0, \"finished\": true, \"failed\": false}\n".to_string(),
        };
        let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
        Response::from_string(text).with_header(header)
    } else if path == "/maps" {
        html_response(maps_page(&state.maps()))
    } else if let Some(rest) = path.strip_prefix("/map/") {
        map_route(&state, rest, &url)
    } else if path == "/favicon.ico" {
        Response::from_data(Vec::new()).with_status_code(204)
    } else if path == "/api/books" {
        json_response(books_json(&shelf))
    } else if path == "/search" {
        let q = query_param(&url, "q").unwrap_or_default();
        html_response(search_page(&shelf, &state.epubs(), &q))
    } else if path == "/api/search" {
        let q = query_param(&url, "q").unwrap_or_default();
        json_response(search_json(&shelf, &q))
    } else if path == "/catalog" {
        let q = query_param(&url, "q").unwrap_or_default();
        let lang = query_param(&url, "lang").unwrap_or_else(|| "eng".to_string());
        let c = query_param(&url, "c").unwrap_or_default();
        let src = query_param(&url, "src").unwrap_or_else(|| "kiwix".to_string());
        html_response(catalog_page(&state, &q, &lang, &c, &src))
    } else if path == "/api/history" {
        let u = query_param(&url, "url").unwrap_or_default();
        let t = query_param(&url, "title").unwrap_or_default();
        record_history(&state, &u, &t);
        Response::from_data(Vec::new()).with_status_code(204)
    } else if path == "/api/bookmark" {
        let u = query_param(&url, "url").unwrap_or_default();
        let t = query_param(&url, "title").unwrap_or_default();
        let on = toggle_bookmark(&state, &u, &t);
        json_response(format!("{{\"bookmarked\":{}}}", on))
    } else if path == "/api/bookmark-check" {
        let u = query_param(&url, "url").unwrap_or_default();
        let on = state
            .data
            .read()
            .unwrap()
            .bookmarks
            .iter()
            .any(|m| m.url == u);
        json_response(format!("{{\"bookmarked\":{}}}", on))
    } else if path == "/bookmark-remove" {
        let u = query_param(&url, "url").unwrap_or_default();
        {
            let mut data = state.data.write().unwrap();
            data.bookmarks.retain(|m| m.url != u);
        }
        state.save_data();
        Response::from_data(Vec::new())
            .with_status_code(302)
            .with_header(Header::from_bytes(&b"Location"[..], &b"/bookmarks"[..]).unwrap())
    } else if path == "/bookmarks" {
        html_response(bookmarks_page(&state))
    } else if path == "/history" {
        html_response(history_page(&state))
    } else if path == "/acquire" {
        let u = query_param(&url, "url").unwrap_or_default();
        let n = query_param(&url, "name").unwrap_or_default();
        let size: u64 = query_param(&url, "size")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let (status, body) = acquire(state.clone(), &u, &n, size);
        html_response(body).with_status_code(status)
    } else if path == "/acquire-progress" {
        html_response(PROGRESS_PAGE.to_string())
    } else if path == "/api/acquire-status" {
        json_response(downloads_json(&state))
    } else if let Some(rest) = path.strip_prefix("/epub/") {
        epub_route(&state, rest)
    } else if let Some(rest) = path.strip_prefix("/book/") {
        let (id_part, item_path) = match rest.split_once('/') {
            Some((a, b)) => (a, b.to_string()),
            None => (rest, String::new()),
        };
        match id_part.parse::<usize>().ok().and_then(|id| shelf.book(id)) {
            Some(book) => {
                if item_path.is_empty() {
                    let location = format!("/book/{}/{}", book.id, book.main_path);
                    Response::from_data(Vec::new())
                        .with_status_code(302)
                        .with_header(
                            Header::from_bytes(&b"Location"[..], location.as_bytes()).unwrap(),
                        )
                } else {
                    let decoded = url_decode(&item_path);
                    match book.get(&decoded) {
                        Ok(Some((mime, bytes))) => Response::from_data(bytes).with_header(
                            Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap(),
                        ),
                        Ok(None) => Response::from_string("not aboard\n").with_status_code(404),
                        Err(e) => Response::from_string(format!("error: {}\n", e))
                            .with_status_code(500),
                    }
                }
            }
            None => Response::from_string("no such book\n").with_status_code(404),
        }
    } else {
        Response::from_string("not found\n").with_status_code(404)
    };

    let _ = request.respond(response);
}


fn load_edition_key(path: &str) -> Result<ed25519_dalek::SigningKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read key file: {e}"))?;
    for line in text.lines() {
        if let Some(hex) = line.strip_prefix("idem-ed25519-secret ") {
            let bytes = sync::hex_decode(hex.trim())
                .filter(|b| b.len() == 32)
                .ok_or("key file secret is not 32 bytes of hex")?;
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            return Ok(ed25519_dalek::SigningKey::from_bytes(&seed));
        }
    }
    Err("key file carries no idem-ed25519-secret line".to_string())
}

/// Publisher side of an edition, run as a subcommand so any machine
/// with the server binary can seal a mirror:
///   coffer-server edition pack <shelf-dir> <mirror-dir> <name> [--key <key-file>]
fn edition_cli(args: &[String]) {
    if args.first().map(|a| a.as_str()) != Some("pack") || args.len() < 4 {
        eprintln!("usage: coffer-server edition pack <shelf-dir> <mirror-dir> <name> [--key <key-file>]");
        std::process::exit(2);
    }
    let shelf = PathBuf::from(&args[1]);
    let mirror = PathBuf::from(&args[2]);
    let name = args[3].clone();
    let key_path = args
        .windows(2)
        .find(|w| w[0] == "--key")
        .map(|w| w[1].clone());
    let chunks_dir = mirror.join("chunks");
    if let Err(e) = std::fs::create_dir_all(&chunks_dir) {
        eprintln!("coffer-server: cannot create mirror: {e}");
        std::process::exit(1);
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&shelf)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|x| {
                            x.eq_ignore_ascii_case("zim")
                                || x.eq_ignore_ascii_case("epub")
                                || x.eq_ignore_ascii_case("pmtiles")
                        })
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    let mut files = Vec::new();
    let mut chunk_count = 0usize;
    for path in &paths {
        let data = match sync::read_file(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("coffer-server: {}: {e}", path.display());
                std::process::exit(1);
            }
        };
        let chunked = sync::chunk_bytes(
            &path.file_name().unwrap_or_default().to_string_lossy(),
            &data,
        );
        for (hash, bytes) in &chunked.chunks {
            let target = chunks_dir.join(hash);
            if !target.exists() {
                if let Err(e) = std::fs::write(&target, bytes) {
                    eprintln!("coffer-server: cannot write chunk: {e}");
                    std::process::exit(1);
                }
            }
            chunk_count += 1;
        }
        println!("- {} - {} chunks", chunked.file.name, chunked.chunks.len());
        files.push(chunked.file);
    }
    let mut edition = sync::Edition {
        format: "coffer-edition/0".to_string(),
        name: name.clone(),
        files,
        signer: None,
        signature: None,
    };
    if let Some(kp) = &key_path {
        match load_edition_key(kp) {
            Ok(key) => {
                use ed25519_dalek::Signer;
                let sig = key.sign(&sync::edition_body(&edition));
                edition.signer = Some(
                    key.verifying_key()
                        .as_bytes()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                );
                edition.signature = Some(
                    sig.to_bytes().iter().map(|b| format!("{b:02x}")).collect(),
                );
            }
            Err(e) => {
                eprintln!("coffer-server: {e}");
                std::process::exit(1);
            }
        }
    }
    let text = serde_json::to_string_pretty(&edition).unwrap();
    if let Err(e) = std::fs::write(mirror.join("edition.json"), text + "\n") {
        eprintln!("coffer-server: cannot write manifest: {e}");
        std::process::exit(1);
    }
    println!(
        "edition {name} packed: {} file(s), {} chunk references, {}",
        edition.files.len(),
        chunk_count,
        if edition.signer.is_some() { "signed" } else { "unsigned" }
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a.as_str()) == Some("edition") {
        edition_cli(&args[2..]);
        return;
    }
    let mut shelf_dir = std::env::var("COFFER_SHELF")
        .or_else(|_| std::env::var("UMIAK_SHELF"))
        .unwrap_or_else(|_| "shelf".to_string());
    let mut port: u16 = 27331;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--shelf" if i + 1 < args.len() => {
                shelf_dir = args[i + 1].clone();
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(port);
                i += 2;
            }
            _ => i += 1,
        }
    }

    // The catalog fetch honors a proxy when the environment names one;
    // on a plain network there is none and the connection is direct.
    let mut agent_builder = ureq::AgentBuilder::new()
        .user_agent("Coffer/0.1 (+https://freecomputation.org)")
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(120));
    if let Ok(connector) = native_tls::TlsConnector::new() {
        agent_builder = agent_builder.tls_connector(std::sync::Arc::new(connector));
    }
    // TLS runs on the platform stack (OpenSSL via ureq's native-tls):
    // it trusts the machine's own certificate store, which is the
    // store the reader's other programs already live by - on networks
    // whose proxy re-signs TLS with a local authority, bundled web
    // roots cannot verify the connection, and the catalog (and every
    // download) fails while the reader's browser succeeds.
    for var in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        if let Ok(p) = std::env::var(var) {
            if let Ok(proxy) = ureq::Proxy::new(&p) {
                agent_builder = agent_builder.proxy(proxy);
                break;
            }
        }
    }

    let mut direct_builder = ureq::AgentBuilder::new()
        .user_agent("Coffer/0.1 (+https://freecomputation.org)")
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(120));
    if let Ok(connector) = native_tls::TlsConnector::new() {
        direct_builder = direct_builder.tls_connector(std::sync::Arc::new(connector));
    }
    let dir = PathBuf::from(&shelf_dir);
    let state = Arc::new(AppState {
        direct_agent: direct_builder.build(),
        data: RwLock::new(load_data(&dir)),
        epubs: RwLock::new(Arc::new(epub::scan_epubs(&dir))),
        maps: RwLock::new(Arc::new(scan_maps(&dir))),
        sync_job: RwLock::new(None),
        shelf: RwLock::new(Arc::new(Shelf::open_dir(&dir))),
        downloads: RwLock::new(Vec::new()),
        cats: RwLock::new(None),
        dir,
        agent: agent_builder.build(),
    });
    println!(
        "coffer-server: {} zim and {} epub book(s) aboard from {}",
        state.shelf().books.len(),
        state.epubs().len(),
        shelf_dir
    );

    let server = Server::http(("127.0.0.1", port)).expect("bind loopback");
    println!("coffer-server: listening on http://127.0.0.1:{}/", port);

    for request in server.incoming_requests() {
        let state = state.clone();
        std::thread::spawn(move || handle(request, state));
    }
}

// ---------------------------------------------------------------------
// Feed parser checks against the feeds' own bytes (trimmed excerpts,
// fetched 2026-11-09). The live fetch paths are exercised by the smoke
// runs; these pin the parsers to the real shapes the sources serve.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutenberg_search_names_book_pages() {
        let xml = include_str!("../tests/fixtures/gutenberg-search.xml");
        let entries = parse_feed_opds(xml);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "page");
        assert_eq!(entries[0].href, "/ebooks/84.opds");
        assert!(entries[0].title.contains("Frankenstein"));
    }

    #[test]
    fn gutenberg_book_page_offers_the_small_epub() {
        let xml = include_str!("../tests/fixtures/gutenberg-book.xml");
        let entries = parse_feed_opds(xml);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "epub");
        assert!(entries[0].href.contains("84.epub.noimages"));
        assert_eq!(entries[0].file_name, "84.epub");
    }

    #[test]
    fn unglue_download_endpoint_names_from_title() {
        let xml = include_str!("../tests/fixtures/unglue.xml");
        let entries = parse_feed_opds(xml);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "epub");
        assert!(entries[0].href.contains("/download_ebook/"));
        assert!(entries[0].file_name.ends_with("-epub.epub"));
    }

    #[test]
    fn sermonindex_entries_name_zim_files() {
        let xml = include_str!("../tests/fixtures/sermon.xml");
        let entries = parse_feed_opds(xml);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "zim");
        assert!(entries[0].file_name.ends_with(".zim"));
    }

    #[test]
    fn openlibrary_publications_come_from_groups() {
        let text = include_str!("../tests/fixtures/openlibrary.json");
        let entries = parse_opds2(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "web");
        assert!(entries[0].href.contains("openlibrary.org/works/"));
    }
}

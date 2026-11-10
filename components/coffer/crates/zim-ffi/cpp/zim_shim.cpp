#include "zim_shim.h"

#include <zim/archive.h>
#include <zim/error.h>
#include <zim/item.h>
#include <zim/search.h>
#include <zim/suggestion.h>

#include <cstdlib>
#include <cstring>
#include <exception>
#include <string>

struct zim_handle {
    zim::Archive archive;
};

static void set_err(char** err_out, const std::string& msg) {
    if (!err_out) return;
    char* p = (char*)std::malloc(msg.size() + 1);
    std::memcpy(p, msg.c_str(), msg.size() + 1);
    *err_out = p;
}

static char* dup_str(const std::string& s) {
    char* p = (char*)std::malloc(s.size() + 1);
    std::memcpy(p, s.c_str(), s.size() + 1);
    return p;
}

zim_handle* coffer_zim_open(const char* path, char** err_out) {
    try {
        return new zim_handle{zim::Archive(std::string(path))};
    } catch (const std::exception& e) {
        set_err(err_out, e.what());
        return nullptr;
    }
}

void coffer_zim_close(zim_handle* h) { delete h; }

uint32_t coffer_zim_entry_count(zim_handle* h) {
    return (uint32_t)h->archive.getEntryCount();
}

uint32_t coffer_zim_article_count(zim_handle* h) {
    return (uint32_t)h->archive.getArticleCount();
}

char* coffer_zim_main_path(zim_handle* h, char** err_out) {
    try {
        auto item = h->archive.getMainEntry().getItem(true);
        return dup_str(item.getPath());
    } catch (const std::exception& e) {
        set_err(err_out, e.what());
        return nullptr;
    }
}

char* coffer_zim_metadata(zim_handle* h, const char* key) {
    try {
        return dup_str(h->archive.getMetadata(key));
    } catch (...) {
        return nullptr;
    }
}

int coffer_zim_get(zim_handle* h, const char* path,
                  uint8_t** out, size_t* out_len,
                  char* mime_buf, size_t mime_cap,
                  char** err_out) {
    try {
        auto entry = h->archive.getEntryByPath(std::string(path));
        auto item = entry.getItem(true);
        auto blob = item.getData();
        uint8_t* buf = (uint8_t*)std::malloc(blob.size() ? blob.size() : 1);
        if (blob.size()) std::memcpy(buf, blob.data(), blob.size());
        *out = buf;
        *out_len = blob.size();
        if (mime_buf && mime_cap) {
            std::strncpy(mime_buf, item.getMimetype().c_str(), mime_cap - 1);
            mime_buf[mime_cap - 1] = '\0';
        }
        return 1;
    } catch (const zim::EntryNotFound&) {
        return 0;
    } catch (const std::exception& e) {
        set_err(err_out, e.what());
        return -1;
    }
}

char* coffer_zim_search(zim_handle* h, const char* query, uint32_t limit,
                       char** err_out) {
    try {
        zim::Searcher searcher(h->archive);
        auto search = searcher.search(zim::Query(std::string(query)));
        auto results = search.getResults(0, (int)limit);
        std::string acc;
        for (auto it = results.begin(); it != results.end(); ++it) {
            acc += it.getPath();
            acc += '\t';
            acc += it.getTitle();
            acc += '\n';
        }
        return dup_str(acc);
    } catch (const std::exception& e) {
        set_err(err_out, e.what());
        return nullptr;
    }
}

char* coffer_zim_suggest(zim_handle* h, const char* query, uint32_t limit,
                        char** err_out) {
    try {
        zim::SuggestionSearcher searcher(h->archive);
        auto search = searcher.suggest(std::string(query));
        auto results = search.getResults(0, (int)limit);
        std::string acc;
        for (auto it = results.begin(); it != results.end(); ++it) {
            acc += (*it).getPath();
            acc += '\t';
            acc += (*it).getTitle();
            acc += '\n';
        }
        return dup_str(acc);
    } catch (const std::exception& e) {
        set_err(err_out, e.what());
        return nullptr;
    }
}

void coffer_zim_free_buf(uint8_t* p) { std::free(p); }
void coffer_zim_free_str(char* p) { std::free(p); }

// Plain C surface over libzim. Every function that can fail takes a
// char** err_out; on failure it returns the documented failure value and
// sets *err_out to a malloc'd message the caller frees with
// umiak_zim_free_str. Strings returned are malloc'd the same way.
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct zim_handle zim_handle;

zim_handle* umiak_zim_open(const char* path, char** err_out);
void umiak_zim_close(zim_handle* h);

uint32_t umiak_zim_entry_count(zim_handle* h);
uint32_t umiak_zim_article_count(zim_handle* h);

// malloc'd path of the archive's main entry (redirects followed), or NULL.
char* umiak_zim_main_path(zim_handle* h, char** err_out);

// malloc'd metadata value for key, or NULL when absent.
char* umiak_zim_metadata(zim_handle* h, const char* key);

// Fetch an item by path. On success returns 1 and sets *out/*out_len to a
// malloc'd copy of the bytes (free with umiak_zim_free_buf) and writes the
// MIME type into mime_buf. Returns 0 when the path is absent, -1 on error
// (err_out set).
int umiak_zim_get(zim_handle* h, const char* path,
                  uint8_t** out, size_t* out_len,
                  char* mime_buf, size_t mime_cap,
                  char** err_out);

// Full-text search. Returns a malloc'd string of newline-joined
// "path<TAB>title" lines, or NULL on error (err_out set, for example when
// the archive carries no full-text index).
char* umiak_zim_search(zim_handle* h, const char* query, uint32_t limit,
                       char** err_out);

// Title suggestions (works without a full-text index). Same wire format
// and failure contract as umiak_zim_search.
char* umiak_zim_suggest(zim_handle* h, const char* query, uint32_t limit,
                        char** err_out);

void umiak_zim_free_buf(uint8_t* p);
void umiak_zim_free_str(char* p);

#ifdef __cplusplus
}
#endif

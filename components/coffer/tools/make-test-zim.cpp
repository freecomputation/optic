// make-test-zim: builds a tiny, fully indexed ZIM used as Coffer's test
// fixture, so the reading path and full-text search can be exercised with
// no downloads. Real shelves carry real corpora; this is three pages.
//
//   g++ -std=c++17 -I third-party/libzim-dev/usr/include \
//       tools/make-test-zim.cpp -o make-test-zim -l:libzim.so.8
//   ./make-test-zim shelf-test/test.zim
//   ./make-test-zim shelf-test/test-noindex.zim noindex
//
// The noindex variant carries no full-text index, like most of the real
// catalog: search against it must fall back to title suggestions.

#include <zim/writer/creator.h>
#include <zim/writer/item.h>

#include <cstring>
#include <iostream>
#include <memory>

static std::shared_ptr<zim::writer::StringItem> page(
        const std::string& path, const std::string& title,
        const std::string& body) {
    std::string html = "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">"
                       "<title>" + title + "</title></head><body><h1>" +
                       title + "</h1><p>" + body + "</p></body></html>";
    return zim::writer::StringItem::create(
        path, "text/html", title,
        zim::writer::Hints{{zim::writer::FRONT_ARTICLE, 1}}, html);
}

int main(int argc, char** argv) {
    if (argc < 2) {
        std::cerr << "usage: make-test-zim <out.zim>\n";
        return 1;
    }
    bool indexing = !(argc > 2 && std::strcmp(argv[2], "noindex") == 0);
    zim::writer::Creator creator;
    if (indexing) creator.configIndexing(true, "eng");
    creator.startZimCreation(argv[1]);

    creator.addMetadata("Title", "Coffer Test Shelf");
    creator.addMetadata("Name", "coffer-test-shelf");
    creator.addMetadata("Language", "eng");
    creator.addMetadata("Creator", "Free Computation");
    creator.addMetadata("Publisher", "Free Computation");
    creator.addMetadata("Description", "Three page fixture for the Coffer reading path.");
    creator.addMetadata("Date", "2026-11-09");

    creator.addItem(page("coffer", "The Coffer",
        "A coffer is the household's strong chest, the box a family "
        "kept its valuables in: books, documents, letters, the deeds "
        "to the land. What mattered was kept at home, under the "
        "owner's own hand, and it travelled when the household "
        "travelled. This browser takes the chest's name because it "
        "keeps the library the same way, aboard and at home, and "
        "asks for no one's permission to open it."));
    creator.addItem(page("vesper", "Vesper, the Evening Star",
        "Vesper is the evening star, the planet Venus seen at dusk, "
        "the first light in the sky and the one sailors steered by "
        "when the day was ending. A star you can navigate by does not "
        "need to be the brightest thing in the sky. It needs to be "
        "there, in its place, every clear evening."));
    creator.addItem(page("offline", "Computing Offline",
        "A computer that only works while a network answers is a "
        "terminal, not a machine of your own. Computing offline means "
        "the work, the knowledge, and the tools are aboard, and the "
        "network, when it comes, is a supply line and never a leash. "
        "Pull the cable. If it stops working, it was never yours."));

    creator.setMainPath("coffer");
    creator.finishZimCreation();
    std::cout << "wrote " << argv[1] << "\n";
    return 0;
}

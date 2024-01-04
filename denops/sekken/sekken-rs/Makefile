all: wasm

model: model.bin.zst

wasm:
	wasm-pack build --weak-refs --reference-types --release --target deno wasm

model.bin.zst: jawiki-latest-pages-articles.xml.bz2
	cargo run --release --bin sekken-make-model < $< > $@

jawiki-latest-pages-articles.xml.bz2:
	wget --no-verbose --no-clobber -O $@ https://dumps.wikimedia.org/jawiki/latest/jawiki-latest-pages-articles.xml.bz2

.PHONY: all wasm model

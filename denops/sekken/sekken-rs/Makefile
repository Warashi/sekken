all: wasm

model: model.bin.zst

wasm:
	wasm-pack build --weak-refs --reference-types --release --target deno wasm

model.bin.zst: data/jawiki-latest-pages-articles.xml.bz2
	cargo run --release --bin sekken-make-model < $< > $@

data/jawiki-latest-pages-articles.xml.bz2:
	mkdir -p data
	wget --no-verbose --no-clobber -O $@ https://dumps.wikimedia.org/jawiki/latest/jawiki-latest-pages-articles.xml.bz2

.PHONY: all wasm model

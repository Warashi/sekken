all: wasm

model: model.bin.zst

wasm:
	wasm-pack build --weak-refs --reference-types --release --target deno wasm

model.bin.zst: data/training-data.txt
	cargo run --release --bin sekken-make-model < $< > $@

data/training-data.txt:
	mkdir -p data
	: > $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00000-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00001-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00002-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00003-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00004-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00005-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00006-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00007-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00008-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00009-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00010-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00011-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00011-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00012-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00013-of-00015.parquet';" >> $@
	duckdb -noheader -s ".mode tabs" -s "SELECT text FROM 'hf://datasets/wikimedia/wikipedia/20231101.ja/train-00014-of-00015.parquet';" >> $@

.PHONY: all wasm model

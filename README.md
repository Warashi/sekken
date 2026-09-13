# sekken

SKK 風の一括変換による Emacs 用日本語入力。

漢字とかなの境界を大文字で示した roman 列（例: `WagahaihaNekodearu.`）を
そのまま打ち、変換キー 1 つで「吾輩は猫である。」に置き換える。
かな・カナなどの内部モードや未確定状態を持たない。

大文字の代わりに `;` でも境界を示せる。`;wagahai;ha;neko;dearu.` は
同じ文になり、リテラルの `;` は `;;` と打つ。

- `sekken-rs/` — 変換エンジン（Rust）。コマンドラインでの変換と、Emacs から使う JSON-RPC サーバー
- `lisp/` — Emacs 側（`sekken-mode`）
- `share/kana-table.tsv` — ローマ字かな変換表。Emacs Lisp とコマンドラインの両方が読む（実体は `sekken-rs/core/kana-table.tsv`）

ローマ字からかなへの変換はエディタ側が行い、エンジンにはかなと変換境界を送る。
入力中のかな表示をエンジンに頼らず出すために、どのエディタもこの変換を持つので、
エンジンは綴りを受け取らない（「エンジンの入力」）。

## 入力の規約

- 大文字で始まる部分が 1 セグメント。セグメントの読みは SKK 辞書で前方一致に引き、残りはかなとして続ける
  - `Nekoha` → 猫は、`Kakimasu` → 書きます
- 送り仮名は SKK と同じく次のセグメントでも表せる（`KaKu` → 書く）
- 大文字を含まなければ、ひらがな（とカタカナ）になる
- 記号は `.` → 。、`,` → 、、`z/` → ・ のように変換表に従う
- `/` で始めた部分は SKK の abbrev。かなにせず、打った綴りをそのまま見出しにして引く（`/emacs` → Emacs、`/GPL` → GNU General Public License）
  - 次の `;` か `/` まで続き、中の大文字は境界にならない
- `>` は SKK の接頭辞・接尾辞の境界。前の部分は `お>` の見出しでも、後の部分は `>かい` の見出しでも引く（`O>Kai` → 御会、`Toukyou>Kai` → 東京会）
  - どちらに付くかは打鍵からは決まらないので、エンジンが両方を候補にして文全体の採点で選ぶ
- `;` `/` `>` の直後の大文字は新しいセグメントを作らず、その部分の先頭の字になる（`;Kai` は `;kai` と同じ）

## エンジンの入力

`sekken server` は標準入出力で JSON-RPC 2.0（LSP と同じ `Content-Length` 区切り）を話す。
`henkan` の params は、エディタがかなにして種類を付けた区間の列。

```json
{"pieces": [{"kind": "convert", "text": "きょう"},
            {"kind": "convert", "text": "は"},
            {"kind": "literal", "text": "Emacs"},
            {"kind": "abbrev", "text": "GPL"},
            {"kind": "convert", "text": "お", "prefix": true},
            {"kind": "convert", "text": "かい", "suffix": true}],
 "top": 10}
```

| kind | 扱い |
|---|---|
| `kana` | 辞書を引かず、かなのまま出す（先頭の小文字など） |
| `convert` | 読みとして SKK 辞書を引く（大文字または `;` で始めた部分） |
| `literal` | 変換せずそのまま出す（英字など。Emacs 側の打ち分けは未実装） |
| `abbrev` | 綴りをそのまま SKK の abbrev の見出しとして引く（`/` で始めた部分） |

`convert` の区間には `>` の印を付けられる。`prefix` が真なら直後に `>` があり
`お>` のような接頭辞の見出しでも引く。`suffix` が真なら直前に `>` があり
`>かい` のような接尾辞の見出しでも引く。省略すれば偽。

区間の種類はエディタが決め、エンジンは文字から推測しない。打ち終えていない末尾の
子音（`Kak` の `k`）は変換せずに送ると、エンジンが送り仮名の子音として扱う。
応答は `{"candidates": ["今日はEmacs", ...]}`。他に `version`、`shutdown`、通知 `exit` がある。

## 準備

エンジンは nix で組む。`./result/bin/sekken` ができる。

```sh
nix build
```

エンジンは 3 つのファイルを必要とする。

| もの | 入手 |
|---|---|
| vibrato 辞書 `system.dic.zst` | https://github.com/daac-tools/vibrato/releases/download/v0.5.0/ipadic-mecab-2_7_0.tar.xz を展開する |
| SKK 辞書 `SKK-JISYO.L` | https://raw.githubusercontent.com/skk-dev/dict/master/SKK-JISYO.L |
| n-gram モデル `model.zst` | https://github.com/Warashi/sekken/releases |

`model.zst` は Wikipedia 日本語版と FineWeb-2 日本語の文から数えた語の bigram で、
同じ Release にある文字言語モデル `lm.zst` は任意。`lm.zst` を渡すと、その提案で格子を
再探索して精度を上げる代わりに変換が遅くなる（使い方は「Emacs での設定」）。
`model.zst` と `lm.zst` はバイナリと同じ Release のものを使う。形式が変わると古い組み合わせは読めない。
学習済みモデルは CC BY-SA 4.0 で、出典と条件は Release に添付した `THIRD-PARTY.md` にある。

### 手元で変換を試す

```sh
./result/bin/sekken henkan --dic system.dic.zst --model model.zst --jisyo SKK-JISYO.L WagahaihaNekodearu.
```

## Emacs での設定

```elisp
(setq sekken-server-program "/path/to/sekken"
      sekken-server-dic "~/.config/sekken/system.dic.zst"
      sekken-server-model "~/.config/sekken/model.zst"
      sekken-server-jisyo "~/.config/sekken/SKK-JISYO.L"
      default-input-method "japanese-sekken")
(require 'sekken)
;; 言語モデルで再探索するなら（1 打鍵あたり 10 ms ほど、文全体の変換で 40〜110 ms ほど遅くなる）
;; (setq sekken-server-lm "~/.config/sekken/lm.zst")
```

`C-\` で sekken を有効・無効にする。有効なバッファでは mode-line 左端に
`-かな:-` が出て、入力中の語が overlay でかな表示され、変換境界は `▽` で示される。
`C-j`（`sekken-convert-key`。`require` より前に設定する）で変換する。
ポイント直前に変換する入力が無ければ、そのキーの元のコマンド（改行など）が動く。

- 大文字を含まない語は、その場でひらがなに置き換わる
- 大文字を含む語は、候補が `completion-in-region` に渡る。corfu を使っていればそのポップアップに出る
- 同じ候補を `completion-at-point-functions` にも提供するので、`corfu-auto` なら入力中にも出る

エンジンは最初の変換時にサブプロセスとして起動し、Emacs 終了時に止まる。
起動後の idle 時に本番と同じ変換を一度通すため、通常は最初の入力より前に
辞書とモデルの読み込みが終わる。打鍵で先読みが中断された場合は次の idle 時に再試行する。
落ちた場合は次の変換で再起動し、連続して落ちたら `M-x sekken-server-restart` を待つ。

## ライセンス

| もの | ライセンス |
|---|---|
| コード（`sekken-rs/`、`lisp/`） | MIT（`LICENSE`） |
| ローマ字かな変換表（`share/kana-table.tsv`） | zlib。skkeleton の変換表を TSV にしたもの |
| 学習済みモデル（model.zst、lm.zst） | CC BY-SA 4.0。Wikipedia 日本語版の本文、FineWeb-2 日本語、zenz-v2.5-dataset から作った派生物 |
| vibrato 辞書（IPA 辞書） | NAIST の独自許諾。再配布には `COPYING` の再掲が要る |
| SKK 辞書（SKK-JISYO.L） | GPL-2.0 以降。実行時にデータとして読むだけで、コードにもモデルにも含まれない |

出典と各許諾の本文は `THIRD-PARTY.md` にまとめている。

## 開発

```sh
nix develop
cd sekken-rs && cargo test && cargo clippy --all-targets
cd lisp && ./test/run.sh
nix flake check
```

実際のエンジンを起動して Emacs から通しで確かめるには、4 つのパスを環境変数で渡す。

```sh
SEKKEN_BIN=./result/bin/sekken SEKKEN_DIC=... SEKKEN_MODEL=... SEKKEN_JISYO=... \
  emacs -Q --batch -L lisp -l lisp/test/e2e.el
```

モデルの学習と評価は別リポジトリ（sekken-train）で行う。

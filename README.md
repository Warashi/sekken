# sekken

SKK 風の一括変換による Emacs 用日本語入力。

漢字とかなの境界を大文字で示した roman 列（例: `WagahaihaNekodearu.`）を
そのまま打ち、変換キー 1 つで「吾輩は猫である。」に置き換える。
入力モードの切り替えや未確定状態を持たない。

- `sekken-rs/` — 変換エンジン（Rust）。コマンドラインでの変換と、Emacs から使う JSON-RPC サーバー
- `lisp/` — Emacs 側（`sekken-mode`）
- `share/kana-table.tsv` — ローマ字かな変換表。Rust と Emacs Lisp の両方が読む（実体は `sekken-rs/core/kana-table.tsv`）

## 入力の規約

- 大文字で始まる部分が 1 セグメント。セグメントの読みは SKK 辞書で前方一致に引き、残りはかなとして続ける
  - `Nekoha` → 猫は、`Kakimasu` → 書きます
- 送り仮名は SKK と同じく次のセグメントでも表せる（`KaKu` → 書く）
- 大文字を含まなければ、ひらがな（とカタカナ）になる
- 記号は `.` → 。、`,` → 、、`z/` → ・ のように変換表に従う

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

同じ Release にある文字言語モデル `lm.zst` は任意で、候補を並べ替えて精度を少し上げる代わりに変換が遅くなる
（使い方は「Emacs での設定」）。
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
      sekken-server-jisyo "~/.config/sekken/SKK-JISYO.L")
(add-hook 'text-mode-hook #'sekken-mode)
;; 言語モデルで並べ替えるなら（1 変換あたり 150 ms ほど遅くなる）
;; (setq sekken-server-lm "~/.config/sekken/lm.zst")
```

`sekken-mode` を有効にしたバッファでは、入力中の語が overlay でかな表示され、
大文字境界は `▽` で示される。`C-j`（`sekken-convert-key`。`require` より前に設定する）で変換する。
ポイント直前に変換する入力が無ければ、そのキーの元のコマンド（改行など）が動く。

- 大文字を含まない語は、その場でひらがなに置き換わる
- 大文字を含む語は、候補が `completion-in-region` に渡る。corfu を使っていればそのポップアップに出る
- 同じ候補を `completion-at-point-functions` にも提供するので、`corfu-auto` なら入力中にも出る

エンジンは最初の変換時にサブプロセスとして起動し、Emacs 終了時に止まる。
落ちた場合は次の変換で再起動し、連続して落ちたら `M-x sekken-server-restart` を待つ。

## ライセンス

| もの | ライセンス |
|---|---|
| コード（`sekken-rs/`、`lisp/`） | MIT（`LICENSE`） |
| ローマ字かな変換表（`share/kana-table.tsv`） | zlib。skkeleton の変換表を TSV にしたもの |
| 学習済みモデル（model.zst、lm.zst） | CC BY-SA 4.0。Wikipedia 日本語版の本文から作った派生物 |
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

# sekken

SKK 風の一括変換による Emacs 用日本語入力。

漢字とかなの境界を大文字で示した roman 列（例: `WagahaihaNekodearu.`）を
そのまま打つと、打鍵に合わせて「吾輩は猫である。」が overlay に見え、
語の終わり（空白や改行）で確定の打鍵なしにそのまま置き換わる。
かな・カナなどの内部モードや未確定状態を持たない。

大文字の代わりに `;` でも境界を示せる。`;wagahai;ha;neko;dearu.` は
同じ文になり、リテラルの `;` は `;;` と打つ。英単語は `'` で開いて
綴りのまま出す（`Kyouha'Emacs;wo;tsukau` → 今日はEmacsを使う）。
`/` は abbrev の入口なので、和文の中の `/` は `//` と打つ。

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
  - 次の `;` `/` `'` まで続き、中の大文字は境界にならない
- `'` で始めた部分は綴りのまま出す（`Kyouha'Emacs;wo;tsukau` → 今日はEmacsを使う）。abbrev と違い辞書を引かないので、`'rust` は ラスト にならず rust のまま
  - 次の `;` `/` `'` まで続き、中の大文字は境界にならない。リテラルの `'` は `''` と打つ（`'don''t` → don't）
- 二重の `;` `/` `'` は、かなの部分でも `'` や `/` の部分でも、その文字 1 つになる（`'a//b` → a/b）
  - 中の空白も語を終えない（`Kyouha'git branch;wo;tsukau` → 今日はgit branchを使う）。英語を数語挟んでも文は 1 語のまま
- `>` は SKK の接頭辞・接尾辞の境界。前の部分は `お>` の見出しでも、後の部分は `>かい` の見出しでも引く（`O>Kai` → 御会、`Toukyou>Kai` → 東京会）
  - どちらに付くかは打鍵からは決まらないので、エンジンが両方を候補にして文全体の採点で選ぶ
- `;` `/` `'` `>` の直後の大文字や `;` は新しいセグメントを作らず、その部分を続ける（`;Kai` は `;kai`、`;o>;kai` は `O>Kai` と同じ）。直後の `/` はその部分を abbrev に、`'` は綴りのままにする

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
| `literal` | 変換せずそのまま出す（`'` で始めた部分） |
| `abbrev` | 綴りをそのまま SKK の abbrev の見出しとして引く（`/` で始めた部分） |

`convert` の区間には `>` の印を付けられる。`prefix` が真なら直後に `>` があり
`お>` のような接頭辞の見出しでも引く。`suffix` が真なら直前に `>` があり
`>かい` のような接尾辞の見出しでも引く。省略すれば偽。

区間の種類はエディタが決め、エンジンは文字から推測しない。エディタは末尾の子音も
母音を待たずに表のとおり送る（`Kak` は かっ、`Kaki` で かき に変わる）。
応答は `{"candidates": ["今日はEmacs", ...]}`。他に `version`、`register`、`shutdown`、通知 `exit` がある。

`register` の params は `{"yomi": "わらし", "surface": "藁市"}`。送りなしの語として
`--user-jisyo` のユーザー辞書に足し、そのファイルに書く。`--user-jisyo` が無い、語に `/` や `;` を
含む、ファイルに書けない、のどれもエラーで断り、エンジンは動き続ける。

## 準備

エンジンは nix で組む。`./result/bin/sekken` ができる。

```sh
nix build
```

エンジンは 4 つのファイルを必要とする。

| もの | 入手 |
|---|---|
| vibrato 辞書 `system.dic.zst` | https://github.com/daac-tools/vibrato/releases/download/v0.5.0/ipadic-mecab-2_7_0.tar.xz を展開する |
| SKK 辞書 `SKK-JISYO.L` | https://raw.githubusercontent.com/skk-dev/dict/master/SKK-JISYO.L |
| n-gram モデル `model.zst` | https://github.com/Warashi/sekken/releases |
| 文字言語モデル `lm.zst` | https://github.com/Warashi/sekken/releases |

`model.zst` は Wikipedia 日本語版と FineWeb-2 日本語の文から数えた語の bigram で、
格子を作るのに使う。`lm.zst` は文字言語モデルで、その提案で格子を再探索して
候補を決める。どちらも必須で、バイナリと同じ Release のものを使う。
形式が変わると古い組み合わせは読めない。
学習済みモデルは CC BY-SA 4.0 で、出典と条件は Release に添付した `THIRD-PARTY.md` にある。

### 手元で変換を試す

```sh
./result/bin/sekken henkan --dic system.dic.zst --model model.zst --jisyo SKK-JISYO.L \
  --lm lm.zst WagahaihaNekodearu.
```

ユーザー辞書があれば `--user-jisyo user-jisyo` も渡す。形式は UTF-8 の SKK-JISYO と同じで、
送りなしの見出しだけを読む。

## Emacs での設定

```elisp
(require 'sekken)
(setopt sekken-server-program "/path/to/sekken"
        sekken-server-dic "~/.config/sekken/system.dic.zst"
        sekken-server-model "~/.config/sekken/model.zst"
        sekken-server-jisyo "~/.config/sekken/SKK-JISYO.L"
        sekken-server-lm "~/.config/sekken/lm.zst"
        default-input-method "japanese-sekken")
```

設定はエンジンの起動時に読むので、`require` との順序は問わない。

`C-\` で sekken を有効・無効にする。有効なバッファでは mode-line 左端に
`-かな:-` が出て、入力中の語が overlay に表示される。入力中の語は、自分が打ち始めた
位置からポイントまでの文字で、もとからバッファにある文字や貼り付けた文字は語に
ならない（`C-j` でも変換できない）。辞書を引く部分のある語は
打鍵に合わせて 1 位候補が見える。打ち足した語の候補が届くまでは、それまでの
1 位候補に打ち足した分のかなを繋いだものが見え、かなには戻らない。辞書を引く
部分の無い語はかなが見え、変換境界は `▽` で示される。

確定の打鍵は無い。語を続けるコマンド（文字の挿入、後退削除、`C-j`、undo）以外の
コマンドは、走る前に見えていたものにそのまま置き換える。置き換えは見えていたものを
そのまま入れるだけで、新しい変換は待たない。候補がまだ届いていなければ、見えていた
かな（打ち足す前の 1 位候補に残りのかなを繋いだもの、または全部かな）のまま
置き換わる。保存や送信の直前にエンジンの応答を待って止まるより、見えていたものが
そのまま入る方を選んでいる。最新の候補が要るときは `C-j` を使う。境界の `▽` は
overlay に見せるだけで、バッファには入らない。空白や改行を打つ、移動する、
送信する、保存する、バッファを切り替える、のどれでも置き換わり、打ったキー本来の
動作は妨げない。語を伸ばす・縮める打鍵では置き換わらない。空白と改行以外の打てる
文字はすべて語の文字なので（`1on1` のような数字混じりの語も 1 語として打てる）、
日本語の文は空白か改行まで 1 語として扱われ、改行で一括して置き換わる。
`'` の綴りの中では空白も語の文字なので、`'git branch` のように英語を数語挟んでも
そこでは置き換わらない。
後退削除は、コマンド名に加えて DEL と backspace に割り当てられたコマンドで見るので、
org-mode のように major mode が張り替えていても語を続ける。別のキーの後退削除は
`sekken-live-continue-commands` に足す。足さないと、そのキーを押した時点で語が
置き換わってから削除が走る。

1 位以外の候補を選ぶには `C-j`（`sekken-mode-map` の `sekken-convert`。別のキーにするなら
`keymap-set sekken-mode-map` で付け替える）を使う。候補が `completion-in-region` に渡るので、corfu を使っていればその
ポップアップに出る。一覧が出ている間は、一覧を動く・絞る・閉じるキーで
語は置き換わらない。候補を選んでそのまま次の語を打ち始められる。ポイント直前に変換する入力が無ければ、そのキーの元の
コマンド（改行など）が動く。

- 辞書を引く部分の無い語（小文字だけ、または `'` の綴りだけ）は、語の終わりでひらがなと綴りに置き換わる（`C-j` でもその場で置き換わる）
- 境界で終わる語（`Neko;`、`'git branch;`）は、末尾の境界を打たなかったものとして手前までが置き換わる（猫、git branch）。境界で終わる語は先読みしないので、置き換わるのは境界の手前までで見えていたものになる。`C-j` なら `O>` を `お>` の見出しで引く。境界だけの語（`;`）は置き換わらず残る
- 置き換わった結果が英字で終わっても（`kyouha'Emacs` → `きょうはEmacs`）、その英字は次の語に含まれない。置き換わった文字を削除しても語には戻らず、続けて打った文字が新しい語になる
- 置き換えを undo で取り消してローマ字に戻せば、その語は入力中に戻り、続きを打つか語の終わりでまた置き換わる。Emacs は打鍵を 20 文字ずつまとめて undo するので、長い語は先頭部分だけ戻り、短い語は 1 回の undo で消える。redo で綴りが戻ったときはカーソルを綴りの末尾へ動かして語に戻す（Emacs の redo はカーソルを挿入位置の先頭に置くため）

エンジンは最初の変換時にサブプロセスとして起動し、Emacs 終了時に止まる。
落ちた場合は次の変換で再起動し、連続して落ちたら `M-x sekken-server-restart` を待つ。
打鍵ごとの先読みが失敗しても打鍵は止まらず（候補が来ないまま語が終われば、
見えていたかなで置き換わる）、最後の失敗の理由は `sekken-convert-last-error` に残る。

### ユーザー辞書

SKK 辞書に無い固有名詞や自分の語彙は `M-x sekken-register` で足す。置き換わった語を
バッファで直し、直した語を region で選んで呼ぶと、読みは最後に置き換えた語のかなが
既定値になる。sekken で打ち直した場合は直した語の読みが既定値になるので、外れた語の
読みは `M-p` の履歴で戻す。region が無ければ語も聞く。読みはローマ字で打ってもかなになり、
大文字や `;` の境界は読みに残らない。登録できるのは送りなしの語だけで、`/` と `;` を
含む語は辞書の形式を壊すので断る。

登録した語はエンジンがユーザー辞書（`sekken-server-user-jisyo`。既定は
`user-emacs-directory` の `sekken-jisyo`）に書き、次の変換から SKK 辞書より先に引く。
登録は、その読みを登録した語に確定した 1 回分として個人化（次の節）にも学習させる。
辞書に足すだけでは、SKK 辞書に同じ読みの語があると助詞付きで打ったときに登録した語が
先に出ないことがあり、外れた語で置き換わった文を個人化が学習した後は単独でも先に出ない。
学習の 1 回分を足すと、単独で打てば外れた語を何度学習した後でも次の変換から登録した語が
先に出る。助詞付きは外れた語の学習が 1 回程度までなら先に出るが、何度も学習した後や
文の中では外れた語が勝つことが残る。その場合は登録した語を `C-j` で一度選べば次から
先に出る。

### 個人化

エンジンの候補で語が確定するたび（語の終わりで 1 位候補に置き換わったとき、
`C-j` で候補を選んだとき）と `M-x sekken-register` で語を登録したとき、
その読みと文をエンジンに送り、`lm.zst` の出力層
（最後の線形層の重みと bias）を 1 歩動かす。動かすのは出力層だけで、他の層と
`model.zst`、辞書は変わらない。誤変換かどうかは見ないので、確定した文はそのまま
学習に回る。候補が届く前にかなのまま置き換わった語と、辞書を引かない語は
エンジンの変換ではないので送らない。歩幅は `--adapt-lr`（既定 3e-3）で決める。
学習は変換とは別のスレッドで走り、変換の応答は待たせない。

動いた出力層は、エンジンをきちんと止めたとき（Emacs の終了、
`M-x sekken-server-restart`）に `sekken-server-adapted-lm`（既定は
`user-emacs-directory` の `sekken-lm.zst`）に書く。次の起動はこのファイルがあれば
`sekken-server-lm` より先に読む。`lm.zst` は書き換えない（配布物が読み取り専用の
場所にあっても困らないようにするため）。ファイルは `lm.zst` と同じ 20 MB ほどになる。
エンジンが落ちた場合はその起動の間に学習した分が消える。

学習した分を捨てるには、このファイルを消す。次の起動からは配布された `lm.zst` に
戻る。`lm.zst` を新しいものに入れ替えたときも消す（古いモデルから作った出力層を
読み続けてしまうため）。このファイルが読めなければ、理由を stderr に出して
`lm.zst` で起動する。`sekken-server-adapted-lm` を nil にすれば個人化しない。

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

実際のエンジンを起動して Emacs から通しで確かめるには、5 つのパスを環境変数で渡す。

```sh
SEKKEN_BIN=./result/bin/sekken SEKKEN_DIC=... SEKKEN_MODEL=... SEKKEN_JISYO=... SEKKEN_LM=... \
  emacs -Q --batch -L lisp -l lisp/test/e2e.el
```

モデルの学習と評価は別リポジトリ（sekken-train）で行う。

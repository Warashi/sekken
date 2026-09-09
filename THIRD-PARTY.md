# 外部の資料とそのライセンス

sekken のコード（`sekken-rs/`、`lisp/`）は MIT License（`LICENSE`）で
提供する。ここには、リポジトリに含む外部由来のデータと、コードには含まれないが
動かすときや学習済みモデルを作るときに使う外部の資料について、それぞれの
出典と許諾条件を記す。

## ローマ字かな変換表（share/kana-table.tsv）

- 出典: [skkeleton](https://github.com/vim-skk/skkeleton) の
  `denops/skkeleton/kana/rom_hira.ts` にある `romToHira`
- 変更点: TSV に書き直し、促音の「次の入力に残す文字」を省き、`z-` の
  変換結果を 〜（U+301C）から ～（U+FF5E）に変えた。他の項目と変換結果は同じ
- 由来の補足: skkeleton の表は ddskk（GPL-3.0 以降）の
  `skk-rom-kana-base-rule-list` に記号などを足した内容と一致する。
  ここでは直接の写し元である skkeleton の許諾に従う
- ライセンス: zlib License。原文を以下に再掲する

```
Copyright (c) 2021 kuuote

This software is provided 'as-is', without any express or implied warranty.
In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

   1. The origin of this software must not be misrepresented; you must not
      claim that you wrote the original software. If you use this software
      in a product, an acknowledgment in the product documentation would be
      appreciated but is not required.

   2. Altered source versions must be plainly marked as such, and must not be
      misrepresented as being the original software.

   3. This notice may not be removed or altered from any source distribution.
```

## 学習済みモデル（model.zst、lm.zst）

学習済みモデルは Wikipedia 日本語版の本文から作った派生物なので、
Creative Commons Attribution-ShareAlike 4.0 International（CC BY-SA 4.0）で
提供する。分かち書きと読みの付与に IPA 辞書（下記）を使っているので、
モデルを配るときは IPA 辞書の著作権表示と免責も併せて示す。
配布先は GitHub Releases で、このファイルを一緒に添付する。

- 変更点: 記事本文から文を取り出し、語の bigram 頻度（model.zst）または
  文字言語モデルの重み（lm.zst）に加工した。本文そのものは含まない。
- ライセンス本文: https://creativecommons.org/licenses/by-sa/4.0/legalcode

## Wikipedia 日本語版（学習データ）

- 出典: Wikipedia 日本語版 https://ja.wikipedia.org/ の CirrusSearch ダンプ
  https://dumps.wikimedia.org/other/cirrussearch/
- 著作者: 各記事の執筆者。各記事の履歴ページに一覧がある
- ライセンス: CC BY-SA 4.0 https://creativecommons.org/licenses/by-sa/4.0/
  （Wikimedia Foundation の利用規約 https://foundation.wikimedia.org/wiki/Policy:Terms_of_Use
  第 7 節に従う）
- 免責: 記事は現状有姿で提供され、Wikimedia Foundation と執筆者はいかなる
  保証もしない
- リポジトリには含めない

## zenz-v2.5-dataset（条件付き学習のペア）

- 出典: https://huggingface.co/datasets/Miwa-Keita/zenz-v2.5-dataset
  （作者: Miwa-Keita。構築は IPA 2024 年度未踏 IT 人材発掘・育成事業の支援による）
- 用途: 読みを条件にする文字言語モデルの学習データ（カタカナ読みと出力のペア）
- ライセンス: `train_wikipedia.jsonl` は 2024 年 2 月取得の Wikipedia 日本語版から
  作られたもので CC BY-SA 4.0。`train_llm-jp-corpus-v3.jsonl` は llm-jp-corpus-v3
  の Common Crawl 由来の部分から作られたもので ODC-BY
  https://opendatacommons.org/licenses/by/1-0/ および Common Crawl の利用規約
  https://commoncrawl.org/terms-of-use に従う
- 変更点: 評価文と重なる行を除き、読みと出力を文字言語モデルの重みに加工した。
  ペアそのものは含まない
- リポジトリには含めない。lm.zst の学習に使ったファイルは Release に記す

## IPA 辞書（ipadic-mecab-2_7_0、vibrato 形式）

- 入手先: https://github.com/daac-tools/vibrato/releases/download/v0.5.0/ipadic-mecab-2_7_0.tar.xz
- 用途: 学習時の分かち書きと読みの付与、変換時の語の分割
- リポジトリには含めない
- 出自: 配布物の `NOTICE` によれば、mecab-ipadic-2.7.0-20070801 のデータを
  vibrato 形式に変換したもので、連接 ID は BCCWJ のコアデータで付け直されている
- ライセンス: 配布物の `COPYING` を以下にそのまま再掲する（再配布の条件として、
  著作権表示と以下の段落を含めることが求められている。末尾の `÷÷` は原本のまま）

```
Copyright 2000, 2001, 2002, 2003 Nara Institute of Science
and Technology.
Copyright 2023, LegalOn Technologies, Inc.
All Rights Reserved.

Use, reproduction, and distribution of this software is permitted.
Any copy of this software, whether in its original form or modified,
must include both the above copyright notice and the following
paragraphs.

Nara Institute of Science and Technology (NAIST),
the copyright holders, disclaims all warranties with regard to this
software, including all implied warranties of merchantability and
fitness, in no event shall NAIST be liable for
any special, indirect or consequential damages or any damages
whatsoever resulting from loss of use, data or profits, whether in an
action of contract, negligence or other tortuous action, arising out
of or in connection with the use or performance of this software.

A large portion of the dictionary entries
originate from ICOT Free Software.  The following conditions for ICOT
Free Software applies to the current dictionary as well.

Each User may also freely distribute the Program, whether in its
original form or modified, to any third party or parties, PROVIDED
that the provisions of Section 3 ("NO WARRANTY") will ALWAYS appear
on, or be attached to, the Program, which is distributed substantially
in the same form as set out herein and that such intended
distribution, if actually made, will neither violate or otherwise
contravene any of the laws and regulations of the countries having
jurisdiction over the User or the intended distribution itself.

NO WARRANTY

The program was produced on an experimental basis in the course of the
research and development conducted during the project and is provided
to users as so produced on an experimental basis.  Accordingly, the
program is provided without any warranty whatsoever, whether express,
implied, statutory or otherwise.  The term "warranty" used herein
includes, but is not limited to, any warranty of the quality,
performance, merchantability and fitness for a particular purpose of
the program and the nonexistence of any infringement or violation of
any right of any third party.

Each user of the program will agree and understand, and be deemed to
have agreed and understood, that there is no warranty whatsoever for
the program and, accordingly, the entire risk arising from or
otherwise connected with the program is assumed by the user.

Therefore, neither ICOT, the copyright holder, or any other
organization that participated in or was otherwise related to the
development of the program and their respective officials, directors,
officers and other employees shall be held liable for any and all
damages, including, without limitation, general, special, incidental
and consequential damages, arising out of or otherwise in connection
with the use or inability to use the program or any product, material
or result produced or otherwise obtained by using the program,
regardless of whether they have been advised of, or otherwise had
knowledge of, the possibility of such damages at any time during the
project or thereafter.  Each user will be deemed to have agreed to the
foregoing by his or her commencement of use of the program.  The term
"use" as used herein includes, but is not limited to, the use,
modification, copying and distribution of the program and the
production of secondary products from the program.

In the case where the program, whether in its original form or
modified, was distributed or delivered to or received by a user from
any person, organization or entity other than ICOT, unless it makes or
grants independently of ICOT any specific warranty to the user in
writing, such person, organization or entity, will also be exempted
from and not be held liable to the user for any such damages as noted
above as far as the program is concerned.
÷÷
```

## SKK 辞書（SKK-JISYO.L）

- 入手先: https://github.com/skk-dev/dict
  （https://raw.githubusercontent.com/skk-dev/dict/master/SKK-JISYO.L）
- 用途: 変換時に読みから候補を引く。実行時にデータとして読むだけで、
  コードにも学習済みモデルにも含まれない
- ライセンス: GNU General Public License version 2 またはそれ以降の版
  https://www.gnu.org/licenses/old-licenses/gpl-2.0.html
- 著作権: Copyright (C) 1988-1995, 1997, 1999-2014 Masahiko Sato ほか、
  SKK Development Team <skk@ring.gr.jp>
- リポジトリには含めない。再配布するときは GPL の条件に従い、
  辞書ファイル先頭の著作権表示を保つこと

## Rust の依存クレート

`sekken-rs/Cargo.lock` にある依存はすべて MIT、Apache-2.0、BSD、Zlib、
Unicode-3.0、Unlicense、BSL-1.0、MPL-2.0 のいずれか（またはその選択）で、
MIT のコードと併せて配れる。一覧は次で得られる。

```sh
cd sekken-rs && cargo tree -f "{p} {l}" --prefix none | sort -u
```

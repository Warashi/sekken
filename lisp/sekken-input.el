;;; sekken-input.el --- 入力中のローマ字の読み方 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 入力中の語のローマ字を変換境界で分け、そこから表示・確定・送信の
;; 文字列を導く。`sekken-input-analyze' が 1 度の分割からその全部を返すので、
;; 表示と確定が別々の解析でずれない。バッファは見ず、文字列だけを相手にする。
;; どこからどこまでが語かは `sekken-word' が決める。
;; 表示用には大文字境界を ▽ で示し、かなにして返す。エンジンには
;; 同じ分割をかなにし、区間の種類を付けた列として送る。
;;
;; 境界は大文字と `;' のほか、abbrev 区間を開く `/'、綴りをそのまま出す
;; literal 区間を開く `'', 接頭辞・接尾辞の印になる `>'。エンジン側
;; （sekken-rs/core/src/segment.rs）も同じ規則で分けるので、規則を変える
;; ときは両方を直す。

;;; Code:

(require 'cl-lib)
(require 'sekken-kana)
(require 'subr-x)

(defun sekken-input-segment (roman)
  "ROMAN を変換境界で分け、(HEAD . SEGMENTS) を返す。
HEAD は先頭の境界より前の文字列。SEGMENTS の各要素は plist で、:text に
小文字化したローマ字（abbrev と literal なら綴りそのまま）、`/' で開いた
区間なら :abbrev t、`'' で開いた区間なら :literal t、直後に `>' があれば
:prefix t、直前に `>' があれば :suffix t を持つ。
大文字と単独のセミコロンが境界で、二重セミコロンはリテラルになる。
二重アポストロフィもどこでもリテラルの `'' になる（`'don''t'）。
境界で開いた直後の大文字や `;' は新しい区間を作らず、その区間を続ける。
開いた直後の `/' はその区間を abbrev に、`'' は literal にする。
abbrev と literal の区間は次の `;' `/' `'' まで続き、中の大文字は境界にしない。"
  (let ((head "")
        (segments nil)
        (current nil)
        ;; 境界で開いたばかりで、まだ文字の無い区間か。
        (fresh nil)
        ;; `/' か `'' で開いた、綴りのまま送る区間の中か。
        (in-spelled nil)
        (index 0))
    (cl-flet ((open (&rest props)
                (when current
                  (push current segments))
                (setq current (append (list :text "") props)))
              (append-text (string)
                (plist-put current :text (concat (plist-get current :text) string))))
      (while (< index (length roman))
        (let ((char (aref roman index)))
          (cond
           ((and (eq char ?')
                 (< (1+ index) (length roman))
                 (eq (aref roman (1+ index)) ?'))
            (if current
                (append-text "'")
              (setq head (concat head "'")))
            (setq fresh nil
                  index (+ index 2)))
           (in-spelled
            (if (memq char '(?\; ?/ ?'))
                (progn
                  (setq in-spelled nil)
                  (open)
                  (setq fresh t))
              (append-text (char-to-string char)))
            (setq index (1+ index)))
           ((and (<= ?A char) (<= char ?Z))
            (unless fresh
              (open))
            (append-text (char-to-string (+ char (- ?a ?A))))
            (setq fresh nil
                  index (1+ index)))
           ((and (eq char ?\;)
                 (< (1+ index) (length roman))
                 (eq (aref roman (1+ index)) ?\;))
            (if current
                (append-text ";")
              (setq head (concat head ";")))
            (setq fresh nil
                  index (+ index 2)))
           ((eq char ?\;)
            (unless fresh
              (open))
            (setq fresh t
                  index (1+ index)))
           ((memq char '(?/ ?'))
            ;; 綴りのまま送る区間は `>' の印を持たない。
            (unless fresh
              (open))
            (setq current (list :text "" (if (eq char ?/) :abbrev :literal) t)
                  in-spelled t
                  fresh nil
                  index (1+ index)))
           ((eq char ?>)
            (when current
              (plist-put current :prefix t))
            (open :suffix t)
            (setq fresh t
                  index (1+ index)))
           (t
            (if current
                (append-text (char-to-string char))
              (setq head (concat head (char-to-string char))))
            (setq fresh nil
                  index (1+ index))))))
      (when current
        (push current segments)))
    (cons head (nreverse segments))))

(defun sekken-input--settle (segmented)
  "SEGMENTED から末尾の空の区間を落として返す。
語の終わりの `;' `/' `'' `>' は前の区間を閉じる意図しか持たないので、
打たなかったものとして扱う。前の区間の接頭辞の印は残す。"
  (let ((segments (cdr segmented)))
    (if (and segments
             (string-empty-p (plist-get (car (last segments)) :text)))
        (cons (car segmented) (butlast segments))
      segmented)))

(defun sekken-input--typing (segmented)
  "SEGMENTED の、まだ打っている途中の区間。先頭の小文字だけなら nil。
末尾の子音を母音待ちとして残すのはこの区間だけで、境界で閉じた区間は
もう待たない（`Nek;' は「ねっ」）。"
  (car (last (cdr segmented))))

(defun sekken-input-has-boundary-p (roman)
  "ROMAN が変換境界（大文字・単独のセミコロン・`/'・`''・`>'）を含むか。"
  (and (cdr (sekken-input-segment roman)) t))

(defun sekken-input--segment-kana (segment typingp)
  "SEGMENT の :text をかなにする。
TYPINGP なら末尾の子音を変換せずに残す（`sekken-kana-display'）。
abbrev と literal の区間は綴りのまま返す。"
  (let ((text (plist-get segment :text)))
    (cond
     ((or (plist-get segment :abbrev) (plist-get segment :literal)) text)
     (typingp (sekken-kana-display text))
     (t (sekken-kana-roman-to-kana text)))))

(defun sekken-input--display (segmented typing)
  "SEGMENTED をかなにし、変換境界を ▽ で示す。TYPING は打っている途中の区間。
abbrev 区間は `/'、literal 区間は `'' を付けて綴りのまま、`>' は区間の後ろに示す。
閉じただけの末尾の区間も ▽ として残し、境界が開いていることを見せる。"
  (let ((head (car segmented)))
    (concat
     (if typing
         (sekken-kana-roman-to-kana head)
       (sekken-kana-display head))
     (mapconcat
      (lambda (segment)
        (concat
         "▽"
         (and (plist-get segment :abbrev) "/")
         (and (plist-get segment :literal) "'")
         (sekken-input--segment-kana segment (eq segment typing))
         (and (plist-get segment :prefix) ">")))
      (cdr segmented) ""))))

(defun sekken-input--pieces (settled typing)
  "SETTLED をエンジンに送る区間の列にする。TYPING は打っている途中の区間。
先頭の小文字は kana、変換境界で始まる各部分は convert、`/' で開いた部分は
abbrev、`'' で開いた部分は literal の区間になり、`>' の印は :prefix と
:suffix で付ける。
TYPING が SETTLED から落ちていれば、最後の区間は打ち終えているので
末尾の子音も変換する。
jsonrpc.el が JSON の配列にするようベクタで返す。"
  (let ((head (car settled))
        (pieces nil))
    (unless (string-empty-p head)
      (push (list :kind "kana"
                  :text (if typing
                            (sekken-kana-roman-to-kana head)
                          (sekken-kana-display head)))
            pieces))
    (dolist (segment (cdr settled))
      (push (append
             (list :kind (cond ((plist-get segment :abbrev) "abbrev")
                               ((plist-get segment :literal) "literal")
                               (t "convert"))
                   :text (sekken-input--segment-kana segment (eq segment typing)))
             (and (plist-get segment :prefix) '(:prefix t))
             (and (plist-get segment :suffix) '(:suffix t)))
            pieces))
    (vconcat (nreverse pieces))))

(defun sekken-input--commit (settled typing)
  "SETTLED を、候補を使わずにバッファへ入れる文字列にする。TYPING は打っている途中の区間。
`sekken-input--display' と同じかなを、境界の印（▽ `/' `'' `>'）だけ落として
つなぐ。印は見せるためのもので、バッファには入れない。"
  (concat
   (if typing
       (sekken-kana-roman-to-kana (car settled))
     (sekken-kana-display (car settled)))
   (mapconcat (lambda (segment)
                (sekken-input--segment-kana segment (eq segment typing)))
              (cdr settled) "")))

(defun sekken-input-analyze (roman)
  "入力中の ROMAN を 1 度だけ解析し、表示と確定の両方に使う plist を返す。
:display  overlay に見せる文字列。
:ready    最後の変換境界より後に入力があるか。先読みしてよいかの判断。
:converts 辞書を引く区間（convert か abbrev）があるか。無ければエンジンに
          送っても文字列をつなぐだけなので、エディタで置き換えられる。
:pieces   エンジンに送る区間のベクタ。
:commit   候補を使わずに語を置き換える文字列。表示から境界の印を落としたもの。

分けた結果の使い分けはここだけにある。表示は閉じただけの末尾の区間を
▽ として残し、確定と送信はそれを落とす（`sekken-input--settle'）。"
  (let* ((segmented (sekken-input-segment roman))
         (typing (sekken-input--typing segmented))
         (settled (sekken-input--settle segmented)))
    (list :display (sekken-input--display segmented typing)
          :ready (and typing (not (string-empty-p (plist-get typing :text))) t)
          :converts (and (cl-some (lambda (segment)
                                    (not (plist-get segment :literal)))
                                  (cdr settled))
                         t)
          :pieces (sekken-input--pieces settled typing)
          :commit (sekken-input--commit settled typing))))

;; 以下は解析結果を 1 つしか使わない呼び出し元のための入口。
;; 同じ ROMAN から 2 つ以上を使うなら `sekken-input-analyze' を 1 度呼ぶ。

(defun sekken-input-display (roman)
  "入力中の ROMAN をかなにし、変換境界を ▽ で示した文字列。"
  (plist-get (sekken-input-analyze roman) :display))

(defun sekken-input-ready-p (roman)
  "ROMAN の最後の変換境界より後に入力があれば non-nil を返す。"
  (plist-get (sekken-input-analyze roman) :ready))

(defun sekken-input-converts-p (roman)
  "ROMAN に辞書を引く区間（convert か abbrev）があるか。"
  (plist-get (sekken-input-analyze roman) :converts))

(defun sekken-input-pieces (roman)
  "入力中の ROMAN をエンジンに送る区間のベクタにする。"
  (plist-get (sekken-input-analyze roman) :pieces))

(defun sekken-input-commit (roman)
  "ROMAN を、候補を使わずにバッファへ入れる文字列にする。"
  (plist-get (sekken-input-analyze roman) :commit))

(provide 'sekken-input)
;;; sekken-input.el ends here

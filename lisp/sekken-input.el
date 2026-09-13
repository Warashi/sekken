;;; sekken-input.el --- ポイント直前の入力の切り出しと表示 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 「入力中の語」は、ポイント直前に連続するローマ字と記号の並び。
;; 表示用には大文字境界を ▽ で示し、かなにして返す。エンジンには
;; 同じ分割をかなにし、区間の種類を付けた列として送る。
;;
;; 境界は大文字と `;' のほか、abbrev 区間を開く `/' と、接頭辞・接尾辞
;; の印になる `>'。エンジン側（sekken-rs/core/src/segment.rs）も同じ規則
;; で分けるので、規則を変えるときは両方を直す。

;;; Code:

(require 'cl-lib)
(require 'sekken-kana)
(require 'subr-x)

(defconst sekken-input--chars "A-Za-z'.,!?:;/>\\[\\]-"
  "入力中の語を構成する文字（`skip-chars-backward' 用）。")

(defun sekken-input-bounds ()
  "ポイント直前の入力中の語の (START . END)。無ければ nil。"
  (let ((end (point))
        (start (save-excursion
                 (skip-chars-backward sekken-input--chars)
                 (point))))
    (when (< start end)
      (cons start end))))

(defun sekken-input-segment (roman)
  "ROMAN を変換境界で分け、(HEAD . SEGMENTS) を返す。
HEAD は先頭の境界より前の文字列。SEGMENTS の各要素は plist で、:text に
小文字化したローマ字（abbrev なら綴りそのまま）、`/' で開いた区間なら
:abbrev t、直後に `>' があれば :prefix t、直前に `>' があれば :suffix t を持つ。
大文字と単独のセミコロンが境界で、二重セミコロンはリテラルになる。
境界で開いた直後の大文字は新しい区間を作らず、その区間の先頭の字になる。
abbrev 区間は次の `;' か `/' まで続き、中の大文字は境界にしない。"
  (let ((head "")
        (segments nil)
        (current nil)
        ;; 境界で開いたばかりで、まだ文字の無い区間か。
        (fresh nil)
        ;; `/' で開いた abbrev 区間の中か。
        (in-abbrev nil)
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
           (in-abbrev
            (if (memq char '(?\; ?/))
                (progn
                  (setq in-abbrev nil)
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
            (open)
            (setq fresh t
                  index (1+ index)))
           ((eq char ?/)
            (open :abbrev t)
            (setq in-abbrev t
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

(defun sekken-input-has-boundary-p (roman)
  "ROMAN が大文字または単独のセミコロンによる変換境界を含むか。"
  (and (cdr (sekken-input-segment roman)) t))

(defun sekken-input-ready-p (roman)
  "ROMAN の最後の変換境界より後に入力があれば non-nil を返す。"
  (let ((segments (cdr (sekken-input-segment roman))))
    (and segments
         (not (string-empty-p (plist-get (car (last segments)) :text))))))

(defun sekken-input--segment-kana (segment lastp)
  "SEGMENT の :text をかなにする。
LASTP なら表示と同じく末尾の子音を変換せずに残す（`sekken-kana-display'）。
abbrev 区間は綴りのまま返す。"
  (let ((text (plist-get segment :text)))
    (cond
     ((plist-get segment :abbrev) text)
     (lastp (sekken-kana-display text))
     (t (sekken-kana-roman-to-kana text)))))

(defun sekken-input-pieces (roman)
  "入力中の ROMAN をエンジンに送る区間の列にする。
先頭の小文字は kana、変換境界で始まる各部分は convert、`/' で開いた部分は
abbrev の区間になり、`>' の印は :prefix と :suffix で付ける。
jsonrpc.el が JSON の配列にするようベクタで返す。"
  (let* ((segmented (sekken-input-segment roman))
         (head (car segmented))
         (segments (cdr segmented))
         (pieces nil))
    (unless (string-empty-p head)
      (push (list :kind "kana"
                  :text (if segments
                            (sekken-kana-roman-to-kana head)
                          (sekken-kana-display head)))
            pieces))
    (while segments
      (let ((segment (car segments)))
        (push (append
               (list :kind (if (plist-get segment :abbrev) "abbrev" "convert")
                     :text (sekken-input--segment-kana segment (null (cdr segments))))
               (and (plist-get segment :prefix) '(:prefix t))
               (and (plist-get segment :suffix) '(:suffix t)))
              pieces))
      (setq segments (cdr segments)))
    (vconcat (nreverse pieces))))

(defun sekken-input-display (roman)
  "入力中の ROMAN をかなにし、変換境界を ▽ で示す。
abbrev 区間は `/' を付けて綴りのまま、`>' は区間の後ろに示す。"
  (let* ((segmented (sekken-input-segment roman))
         (head (car segmented))
         (segments (cdr segmented))
         (display
          (if segments
              (sekken-kana-roman-to-kana head)
            (sekken-kana-display head))))
    (concat
     display
     (mapconcat
      (lambda (segment)
        (concat
         "▽"
         (and (plist-get segment :abbrev) "/")
         (sekken-input--segment-kana segment (eq segment (car (last segments))))
         (and (plist-get segment :prefix) ">")))
      segments ""))))

(provide 'sekken-input)
;;; sekken-input.el ends here

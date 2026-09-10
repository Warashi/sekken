;;; sekken-input.el --- ポイント直前の入力の切り出しと表示 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 「入力中の語」は、ポイント直前に連続するローマ字と記号の並び。
;; 表示用には大文字境界を ▽ で示し、かなにして返す。

;;; Code:

(require 'sekken-kana)
(require 'subr-x)

(defconst sekken-input--chars "A-Za-z'.,!?:;\\[\\]-"
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
  "ROMAN を変換境界で分け、(PREFIX . SEGMENTS) を返す。
大文字と単独のセミコロンが境界で、二重セミコロンはリテラルになる。"
  (let ((prefix "")
        (segments nil)
        (current nil)
        (index 0))
    (while (< index (length roman))
      (let ((char (aref roman index)))
        (cond
         ((and (<= ?A char) (<= char ?Z))
          (when current
            (push current segments))
          (setq current (char-to-string (+ char (- ?a ?A)))
                index (1+ index)))
         ((eq char ?\;)
          (if (and (< (1+ index) (length roman))
                   (eq (aref roman (1+ index)) ?\;))
              (progn
                (if current
                    (setq current (concat current ";"))
                  (setq prefix (concat prefix ";")))
                (setq index (+ index 2)))
            (when current
              (push current segments))
            (setq current ""
                  index (1+ index))))
         (t
          (if current
              (setq current (concat current (char-to-string char)))
            (setq prefix (concat prefix (char-to-string char))))
          (setq index (1+ index))))))
    (when current
      (push current segments))
    (cons prefix (nreverse segments))))

(defun sekken-input-has-boundary-p (roman)
  "ROMAN が大文字または単独のセミコロンによる変換境界を含むか。"
  (and (cdr (sekken-input-segment roman)) t))

(defun sekken-input-ready-p (roman)
  "ROMAN の最後の変換境界より後に入力があれば non-nil を返す。"
  (let ((segments (cdr (sekken-input-segment roman))))
    (and segments
         (not (string-empty-p (car (last segments)))))))

(defun sekken-input-display (roman)
  "入力中の ROMAN をかなにし、変換境界を ▽ で示す。"
  (let* ((segmented (sekken-input-segment roman))
         (prefix (car segmented))
         (segments (cdr segmented))
         (display
          (if segments
              (sekken-kana-roman-to-kana prefix)
            (sekken-kana-display prefix))))
    (concat
     display
     (mapconcat
      (lambda (segment)
        (concat
         "▽"
         (if (eq segment (car (last segments)))
             (sekken-kana-display segment)
           (sekken-kana-roman-to-kana segment))))
      segments ""))))

(provide 'sekken-input)
;;; sekken-input.el ends here

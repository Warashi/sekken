;;; sekken-input.el --- ポイント直前の入力の切り出しと表示 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; 「入力中の語」は、ポイント直前に連続するローマ字と記号の並び。
;; 表示用には大文字境界を ▽ で示し、かなにして返す。

;;; Code:

(require 'sekken-kana)

(defconst sekken-input--chars "A-Za-z'.,!?:\\[\\]-"
  "入力中の語を構成する文字（`skip-chars-backward' 用）。")

(defun sekken-input-bounds ()
  "ポイント直前の入力中の語の (START . END)。無ければ nil。"
  (let ((end (point))
        (start (save-excursion
                 (skip-chars-backward sekken-input--chars)
                 (point))))
    (when (< start end)
      (cons start end))))

(defun sekken-input-has-upper-p (roman)
  "ROMAN が大文字境界を含むか。"
  ;; `case-fold-search' が t だと [A-Z] が小文字にも一致する。
  (let ((case-fold-search nil))
    (and (string-match-p "[A-Z]" roman) t)))

(defun sekken-input-display (roman)
  "入力中の ROMAN の表示文字列。大文字境界を ▽ で示し、かなにする。"
  (let ((parts nil)
        (start 0)
        (len (length roman)))
    (dotimes (i len)
      (when (and (> i start) (<= ?A (aref roman i) ?Z))
        (push (substring roman start i) parts)
        (setq start i)))
    (push (substring roman start len) parts)
    (setq parts (nreverse parts))
    (mapconcat
     (lambda (part)
       (let* ((upper (and (> (length part) 0) (<= ?A (aref part 0) ?Z)))
              (roman (if upper (downcase part) part))
              (last (eq part (car (last parts))))
              (kana (if last
                        (sekken-kana-display roman)
                      (sekken-kana-roman-to-kana roman))))
         (if upper (concat "▽" kana) kana)))
     parts "")))

(provide 'sekken-input)
;;; sekken-input.el ends here

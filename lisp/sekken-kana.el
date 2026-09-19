;;; sekken-kana.el --- ローマ字からかなへの変換 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; Rust 側と同じ share/kana-table.tsv（実体は sekken-rs/core/kana-table.tsv）を読み、最長一致で変換する。
;; 入力中のかな表示はこのファイルだけで完結し、エンジンを呼ばない。

;;; Code:

(require 'cl-lib)

(defconst sekken-kana--table-file
  (expand-file-name "../share/kana-table.tsv"
                    (file-name-directory (or load-file-name buffer-file-name)))
  "ローマ字かな変換表のパス。")

(defvar sekken-kana--table nil
  "ローマ字文字列からかなへのハッシュ表。")

(defvar sekken-kana--max-key 1
  "変換表のキーの最大長。最長一致の探索幅。")

(defvar sekken-kana--keys-by-last nil
  "末尾の文字から、その文字で終わる 2 文字以上のキーの並びへのハッシュ表。
`sekken-kana-completes-key-p' が引く。")

(defun sekken-kana-load-table (&optional file)
  "FILE（既定は `sekken-kana--table-file'）から変換表を読み込む。"
  (let ((table (make-hash-table :test #'equal))
        (keys-by-last (make-hash-table))
        (max-key 1))
    (with-temp-buffer
      (insert-file-contents (or file sekken-kana--table-file))
      (goto-char (point-min))
      (while (re-search-forward "^\\([^\t\n]+\\)\t\\([^\t\n]+\\)$" nil t)
        (let ((roman (match-string 1)))
          (setq max-key (max max-key (length roman)))
          (puthash roman (match-string 2) table)
          (when (> (length roman) 1)
            (push roman (gethash (aref roman (1- (length roman))) keys-by-last))))))
    (setq sekken-kana--table table
          sekken-kana--keys-by-last keys-by-last
          sekken-kana--max-key max-key)
    table))

(defun sekken-kana--table ()
  "変換表を返す。未読み込みなら読む。"
  (or sekken-kana--table (sekken-kana-load-table)))

(defun sekken-kana-completes-key-p (roman char)
  "ROMAN の末尾に CHAR を足すと、2 文字以上の変換表のキーが完成するか。
境界や語の終わりの文字を、直前までと合わせて表のキーとして読むかを決める
（`z' の後の `/' は境界でなく ・ のキー）。"
  (sekken-kana--table)
  (cl-some (lambda (key)
             (string-suffix-p (substring key 0 -1) roman))
           (gethash char sekken-kana--keys-by-last)))

(defun sekken-kana-roman-to-kana (roman)
  "ROMAN をかなにする。表に無い部分はそのまま残す。"
  (let ((table (sekken-kana--table))
        (pos 0)
        (len (length roman))
        (parts nil))
    (while (< pos len)
      (let ((n (min sekken-kana--max-key (- len pos)))
            (found nil))
        (while (and (> n 0) (not found))
          (let ((kana (gethash (substring roman pos (+ pos n)) table)))
            (if kana
                (setq found kana
                      pos (+ pos n))
              (setq n (1- n)))))
        (unless found
          (setq found (substring roman pos (1+ pos))
                pos (1+ pos)))
        (push found parts)))
    (apply #'concat (nreverse parts))))

(provide 'sekken-kana)
;;; sekken-kana.el ends here

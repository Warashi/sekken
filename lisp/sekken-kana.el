;;; sekken-kana.el --- ローマ字からかなへの変換 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; Rust 側と同じ share/kana-table.tsv（実体は sekken-rs/core/kana-table.tsv）を読み、最長一致で変換する。
;; 入力中のかな表示はこのファイルだけで完結し、エンジンを呼ばない。

;;; Code:

(defconst sekken-kana--table-file
  (expand-file-name "../share/kana-table.tsv"
                    (file-name-directory (or load-file-name buffer-file-name)))
  "ローマ字かな変換表のパス。")

(defvar sekken-kana--table nil
  "ローマ字文字列からかなへのハッシュ表。")

(defvar sekken-kana--max-key 1
  "変換表のキーの最大長。最長一致の探索幅。")

(defun sekken-kana-load-table (&optional file)
  "FILE（既定は `sekken-kana--table-file'）から変換表を読み込む。"
  (let ((table (make-hash-table :test #'equal))
        (max-key 1))
    (with-temp-buffer
      (insert-file-contents (or file sekken-kana--table-file))
      (goto-char (point-min))
      (while (re-search-forward "^\\([^\t\n]+\\)\t\\([^\t\n]+\\)$" nil t)
        (let ((roman (match-string 1)))
          (setq max-key (max max-key (length roman)))
          (puthash roman (match-string 2) table))))
    (setq sekken-kana--table table
          sekken-kana--max-key max-key)
    table))

(defun sekken-kana--table ()
  "変換表を返す。未読み込みなら読む。"
  (or sekken-kana--table (sekken-kana-load-table)))

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

(defun sekken-kana-display (roman)
  "入力途中の ROMAN を表示用にかなにする。
末尾の子音 1 文字は次の母音を待っている途中なので変換せず残す。
`n' は単独で「ん」になるため例外とする。"
  (let ((len (length roman)))
    (if (and (> len 0)
             (string-match-p "[b-df-mp-z]" (substring roman (1- len))))
        (concat (sekken-kana-roman-to-kana (substring roman 0 (1- len)))
                (substring roman (1- len)))
      (sekken-kana-roman-to-kana roman))))

(provide 'sekken-kana)
;;; sekken-kana.el ends here

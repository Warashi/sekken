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
  "ローマ字文字列から (かな . 読み直す文字数) へのハッシュ表。
読み直す文字数は、キーの末尾のうち次のキーの先頭として読み直す分。")

(defvar sekken-kana--end-table nil
  "区間を終えるときだけ使う、ローマ字文字列からかなへのハッシュ表。")

(defvar sekken-kana--max-key 1
  "変換表のキーの最大長。最長一致の探索幅。")

(defun sekken-kana-load-table (&optional file)
  "FILE（既定は `sekken-kana--table-file'）から変換表を読み込む。
省略できる 3 列目はキーの末尾のうち次のキーの先頭として読み直す文字で
（`kk\\tっ\\tk'）、キーの末尾でなければその行を使わない。
4 列目が `end' の行は区間を終えるときだけ使う（`n\\tん\\t\\tend'、
`sekken-kana-roman-to-kana-end'）。"
  (let ((table (make-hash-table :test #'equal))
        (end-table (make-hash-table :test #'equal))
        (max-key 1))
    (with-temp-buffer
      (insert-file-contents (or file sekken-kana--table-file))
      (goto-char (point-min))
      (while (re-search-forward
              "^\\([^\t\n]+\\)\t\\([^\t\n]+\\)\\(?:\t\\([^\t\n]*\\)\\(?:\t\\([^\t\n]+\\)\\)?\\)?$"
              nil t)
        (let ((roman (match-string 1))
              (reread (or (match-string 3) "")))
          (cond
           ((equal (match-string 4) "end")
            (when (string-empty-p reread)
              (puthash roman (match-string 2) end-table)))
           ;; 読み直す文字がキー全体だと位置が進まない。
           ((and (string-suffix-p reread roman)
                 (> (length roman) (length reread)))
            (setq max-key (max max-key (length roman)))
            (puthash roman (cons (match-string 2) (length reread)) table))))))
    (setq sekken-kana--table table
          sekken-kana--end-table end-table
          sekken-kana--max-key max-key)
    table))

(defun sekken-kana--table ()
  "変換表を返す。未読み込みなら読む。"
  (or sekken-kana--table (sekken-kana-load-table)))

(defun sekken-kana-key-across-p (before after)
  "BEFORE の末尾と AFTER の先頭にまたがる変換表のキーがあるか。
BEFORE と AFTER の境目が最長一致のキーの途中に当たるかを決める。境界や
語の終わりの文字を直前までと合わせてキーとして読むか（`z' の後の `/' は
境界でなく ・ のキー）、先頭部分の候補に残りのかなを繋いでよいかに使う。"
  (let ((table (sekken-kana--table)))
    (and
     (cl-loop for left from 1 to (min (length before) (1- sekken-kana--max-key))
              thereis
              (let ((prefix (substring before (- (length before) left))))
                (cl-loop for right from 1 to (min (length after)
                                                  (- sekken-kana--max-key left))
                         thereis (gethash (concat prefix (substring after 0 right))
                                          table))))
     t)))

(defun sekken-kana--convert (roman)
  "ROMAN をかなにし、(かな 残った末尾の開始位置 最後に当たったキーの開始位置) を返す。
かなには、どのキーにも当たらず末尾に残った文字を含めない。キーに当たらなければ
最後のキーの開始位置は nil。"
  (let ((table (sekken-kana--table))
        (pos 0)
        (unmatched 0)
        (last nil)
        (len (length roman))
        (parts nil))
    (while (< pos len)
      (let ((n (min sekken-kana--max-key (- len pos)))
            (found nil))
        (while (and (> n 0) (not found))
          (let ((output (gethash (substring roman pos (+ pos n)) table)))
            (if output
                (setq found output)
              (setq n (1- n)))))
        (if (not found)
            (setq pos (1+ pos))
          (push (substring roman unmatched pos) parts)
          (push (car found) parts)
          (setq last pos
                pos (+ pos (- n (cdr found)))
                unmatched pos))))
    (list (apply #'concat (nreverse parts)) unmatched last)))

(defun sekken-kana-roman-to-kana (roman)
  "ROMAN をかなにする。表に無い部分はそのまま残す。"
  (let ((converted (sekken-kana--convert roman)))
    (concat (car converted) (substring roman (nth 1 converted)))))

(defun sekken-kana-roman-to-kana-end (roman)
  "区間を終えるときの ROMAN をかなにする。
`sekken-kana-roman-to-kana' に加えて、どのキーにも当たらず残った末尾が
4 列目 `end' の行のキーで終わっていれば、そのかなにする。"
  (sekken-kana--table)
  (let* ((converted (sekken-kana--convert roman))
         (tail (substring roman (nth 1 converted))))
    ;; 最長のキーを使うため、末尾の先頭寄りから探す。
    (concat
     (car converted)
     (or (cl-loop for start from 0 below (length tail)
                  for kana = (gethash (substring tail start) sekken-kana--end-table)
                  when kana return (concat (substring tail 0 start) kana))
         tail))))

(defun sekken-kana-last-unit-length (roman)
  "ROMAN の末尾のかな 1 つ分の文字数。ROMAN が空なら 0。
どのキーにも当たらず末尾に残った文字があれば、その 1 文字。無ければ最後に
当たったキーのうち、次のキーとして読み直さない部分。"
  (let ((converted (sekken-kana--convert roman))
        (len (length roman)))
    (cond
     ((< (nth 1 converted) len) 1)
     ((nth 2 converted) (- len (nth 2 converted)))
     (t 0))))

(provide 'sekken-kana)
;;; sekken-kana.el ends here

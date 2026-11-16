;;; sekken-typo.el --- 読めない綴りを直した読みの候補 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; どのキーにもならないと決まった英字（`knaji' の k）を打ち間違いとみなし、
;; その周りの入れ替わり・余分な 1 文字・隣接キー（QWERTY）を直して英字が
;; 残らなくなる読みを並べる。ローマ字は 1 文字の直しのほとんどがかなになり、
;; 綴りだけでは 1 つに絞れないので、選ぶのはエンジンに任せる。

;;; Code:

(require 'cl-lib)
(require 'sekken-kana)

(defconst sekken-typo--rows '("qwertyuiop" "asdfghjkl" "zxcvbnm")
  "QWERTY の文字キーの段。")

(defun sekken-typo--neighbors (char)
  "QWERTY で CHAR の隣にある文字キー。同じ段の左右、上の段、下の段の順。"
  (let ((rows sekken-typo--rows)
        same other)
    (cl-loop for row in rows
             for r from 0
             for c = (cl-position char row)
             when c
             do (setq same (delq nil (list (and (> c 0) (aref row (1- c)))
                                           (and (< (1+ c) (length row))
                                                (aref row (1+ c))))))
             (dolist (dr '(-1 1))
               (let ((near (and (<= 0 (+ r dr) 2) (nth (+ r dr) rows))))
                 (when near
                   ;; 下の段は半キー右にずれるので、上の段では同じ列と右、下の段では
                   ;; 左と同じ列が隣になる。
                   (dolist (dc (if (< dr 0) '(0 1) '(-1 0)))
                     (let ((k (+ c dc)))
                       (when (and (<= 0 k) (< k (length near)))
                         (push (aref near k) other))))))))
    (append same (nreverse other))))

(defvar sekken-typo--prefixes nil
  "変換表と、そのキーのキー全体より短い先頭部分の集合の組。")

(defun sekken-typo--prefix-p (roman)
  "ROMAN が変換表のキーの、キー全体より短い先頭部分か。"
  (let ((table (sekken-kana--table)))
    (unless (eq (car sekken-typo--prefixes) table)
      (let ((prefixes (make-hash-table :test #'equal)))
        (maphash (lambda (key _)
                   (dotimes (n (length key))
                     (puthash (substring key 0 n) t prefixes)))
                 table)
        (setq sekken-typo--prefixes (cons table prefixes))))
    (gethash roman (cdr sekken-typo--prefixes))))

(defun sekken-typo--stuck (roman)
  "ROMAN で、どのキーにもならないと決まった最初の英字の位置。無ければ nil。
`sekken-kana-roman-to-kana' と同じ最長一致で読み、末尾に残った文字のうち、
まだキーの先頭部分として続き得るものは含めない。"
  (let ((table (sekken-kana--table))
        (len (length roman))
        (pos 0)
        (found nil))
    (while (and (< pos len) (not found))
      (let ((n (min sekken-kana--max-key (- len pos)))
            (hit nil))
        (while (and (> n 0) (not hit))
          (setq hit (gethash (substring roman pos (+ pos n)) table))
          (unless hit (setq n (1- n))))
        (cond
         (hit (setq pos (+ pos (- n (cdr hit)))))
         ((and (<= ?a (aref roman pos) ?z)
               (not (sekken-typo--prefix-p (substring roman pos))))
          (setq found pos))
         (t (setq pos (1+ pos))))))
    found))

(defun sekken-typo--edits (roman i)
  "ROMAN の I 番目で詰まった英字の周りを 1 か所直した綴りの列。"
  (let ((len (length roman))
        (result nil))
    (cl-flet ((edit (from to text)
                (when (and (<= 0 from) (<= to len))
                  (push (concat (substring roman 0 from) text (substring roman to))
                        result))))
      ;; 入れ替わり: 詰まった英字とその前後の隣り合う 2 文字。
      (dolist (p (list (1+ i) i (1- i)))
        (when (and (<= 0 p) (< (1+ p) len))
          (edit p (+ p 2) (string (aref roman (1+ p)) (aref roman p)))))
      ;; 余分な 1 文字: 詰まった英字か次の文字。
      (dolist (p (list (1+ i) i))
        (when (< p len) (edit p (1+ p) "")))
      ;; 隣接キー: 詰まった英字か次の文字を隣のキーに。
      (dolist (p (list (1+ i) i))
        (when (and (< p len) (<= ?a (aref roman p) ?z))
          (dolist (c (sekken-typo--neighbors (aref roman p)))
            (edit p (1+ p) (string c))))))
    (nreverse result)))

(defun sekken-typo-alternatives (roman)
  "ROMAN に読めない英字があれば、直して英字が残らなくなる読みの列。無ければ nil。
読みは区間を終えるときの変換（`sekken-kana-roman-to-kana-end'）で作る。"
  (let ((i (sekken-typo--stuck roman)))
    (when i
      (delete-dups
       (cl-loop for edited in (sekken-typo--edits roman i)
                for kana = (sekken-kana-roman-to-kana-end edited)
                unless (string-match-p "[a-z]" kana)
                collect kana)))))

(provide 'sekken-typo)
;;; sekken-typo.el ends here

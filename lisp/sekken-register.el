;;; sekken-register.el --- ユーザー辞書への語の登録 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; `sekken-register' は、読みと語をエンジンのユーザー辞書に足す。
;; 外れた語はどのみちバッファで直すので、直した語を region で選んで
;; 呼べば語は打ち直さない。読みは最後に置き換えた語のかなを既定値に
;; するので、置き換わった直後なら読みも打ち直さない。読みはローマ字で
;; 打ってもかなにする。登録は送りなしの語だけ。

;;; Code:

(require 'sekken-convert)
(require 'sekken-input)
(require 'sekken-server)

(defun sekken-register-yomi (string)
  "STRING を登録する読みにする。
ローマ字ならかなにし、大文字や `;' の境界は読みに残さない。かなはそのまま。"
  (mapconcat (lambda (piece) (plist-get piece :text))
             (sekken-input-pieces string)
             ""))

(defun sekken-register-default-yomi ()
  "読みの既定値。このバッファで最後に置き換えた語のかな。無ければ nil。"
  (let ((roman (sekken-input-finished-roman)))
    (and roman (sekken-register-yomi roman))))

;;;###autoload
(defun sekken-register (yomi surface)
  "読み YOMI の語 SURFACE をユーザー辞書に登録する。
対話的には、region があればその文字列を語にし、無ければ語を聞く。
読みは最後に置き換えた語のかなを既定値にして聞く。ローマ字で打ってもよい。
登録した後は覚えている候補を捨て、次の変換から登録した語が出る。"
  (interactive
   (let* ((surface (if (use-region-p)
                       (buffer-substring-no-properties (region-beginning)
                                                       (region-end))
                     (read-string "登録する語: ")))
          (default (sekken-register-default-yomi)))
     (list (read-string (format-prompt "読み" default) nil nil default)
           surface)))
  (let ((yomi (sekken-register-yomi yomi)))
    (sekken-server-register yomi surface)
    (sekken-convert-forget-all)
    (message "sekken: %s /%s/ を登録しました" yomi surface)))

(provide 'sekken-register)
;;; sekken-register.el ends here

;;; sekken-register.el --- ユーザー辞書への語の登録 -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; `sekken-register' は、読みと語をエンジンのユーザー辞書に足す。
;; 外れた語はどのみちバッファで直すので、直した語を region で選んで
;; 呼べば語は打ち直さない。読みは最後に置き換えた語のかなを既定値に
;; し、それより前に置き換えた語は履歴（M-p）で辿れる。sekken で打ち
;; 直してから登録しても、外れた語の読みに戻れる。読みはローマ字で
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

(defun sekken-register-recent-yomi ()
  "このバッファで置き換えた語の読み。新しい順で、同じ読みは 1 つ。
末尾の記号（`Sawadadazai.' の 。）は見出しにならないので落とす。"
  (seq-uniq
   (delq nil
         (mapcar (lambda (roman)
                   (let* ((yomi (sekken-register-yomi roman))
                          (trimmed (string-trim-right yomi "[^ぁ-ゖー]+")))
                     (and (not (string-empty-p trimmed)) trimmed)))
                 (sekken-input-finished-romans)))))

(defvar sekken-register-yomi-history nil
  "`sekken-register' が読みを聞くときの履歴。呼ぶたびに置き換えた語の読みにする。")

;;;###autoload
(defun sekken-register (yomi surface)
  "読み YOMI の語 SURFACE をユーザー辞書に登録する。
対話的には、region があればその文字列を語にし、無ければ語を聞く。
読みは最後に置き換えた語のかなを既定値にして聞き、それより前に置き換えた
語の読みは履歴で辿れる。ローマ字で打ってもよい。
登録した後は覚えている候補を捨て、次の変換から登録した語が出る。"
  (interactive
   (let* ((surface (if (use-region-p)
                       (buffer-substring-no-properties (region-beginning)
                                                       (region-end))
                     (read-string "登録する語: ")))
          (recent (sekken-register-recent-yomi))
          (default (car recent)))
     (setq sekken-register-yomi-history (cdr recent))
     (list (read-string (format-prompt "読み" default)
                        nil 'sekken-register-yomi-history default)
           surface)))
  (let ((yomi (sekken-register-yomi yomi)))
    (sekken-server-register yomi surface)
    (sekken-convert-forget-all)
    (message "sekken: %s /%s/ を登録しました" yomi surface)))

(provide 'sekken-register)
;;; sekken-register.el ends here

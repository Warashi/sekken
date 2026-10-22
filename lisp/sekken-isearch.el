;;; sekken-isearch.el --- isearch で sekken の入力を検索文字列に足す -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT

;;; Commentary:
;; isearch は入力方式を 1 回呼んで返った文字だけを足し、すぐミニバッファを
;; 閉じる（`isearch-with-input-method'）。sekken は文字を返さず、overlay と
;; hook で語を変換するので、それでは何も足されない。sekken が有効なときは、
;; 打った鍵から入力方式を引き継いだミニバッファで読み、RET で確定した文字列を
;; 検索文字列に足す。

;;; Code:

(require 'isearch)
(require 'sekken-im)

(defun sekken-isearch--read (last-char)
  "LAST-CHAR から sekken を有効にしたミニバッファで読んだ文字列。C-g なら nil。"
  (let ((overriding-terminal-local-map nil)
        (unread-command-events (cons last-char unread-command-events))
        (prompt (concat (isearch-message-prefix) isearch-message)))
    ;; ミニバッファのプロンプトの性質と重なると read-only で入力できなくなる。
    (set-text-properties 0 (length prompt) nil prompt)
    (condition-case nil
        (read-string prompt nil t nil t)
      (quit nil))))

(defun sekken-isearch-process-characters (process last-char &optional count)
  "sekken が有効なら LAST-CHAR から読んだ文字列を COUNT 回検索に足す。
そうでなければ PROCESS に任せる。`isearch-process-search-multibyte-characters' の advice。"
  (if (and (equal current-input-method sekken-im-name)
           (eq this-command 'isearch-printing-char))
      (let ((string (sekken-isearch--read last-char)))
        (if (seq-empty-p string)
            (isearch-update)
          (let ((string (apply #'concat
                               (make-list (max 1 (or count 1)) string))))
            (isearch-process-search-string string string))))
    (funcall process last-char count)))

(advice-add 'isearch-process-search-multibyte-characters
            :around #'sekken-isearch-process-characters)

(provide 'sekken-isearch)
;;; sekken-isearch.el ends here

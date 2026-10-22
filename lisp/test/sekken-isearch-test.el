;;; sekken-isearch-test.el --- sekken isearch tests -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken)

(defmacro sekken-isearch-test--with-search (reader &rest body)
  "sekken を有効にした buffer で isearch を始め、`read-string' を READER にして BODY を評価する。"
  (declare (indent 1))
  `(with-temp-buffer
     (set-window-buffer nil (current-buffer))
     (insert "今日は日本語の検索")
     (goto-char (point-min))
     (activate-input-method "japanese-sekken")
     (cl-letf (((symbol-function 'read-string) ,reader))
       (isearch-mode t)
       (unwind-protect
           (let ((this-command 'isearch-printing-char))
             ,@body)
         (isearch-done)))))

(ert-deftest sekken-isearch/打った鍵から_sekken_で読んだ文字列を検索に足す ()
  (let (seen)
    (sekken-isearch-test--with-search
        (lambda (prompt &optional _initial _history _default inherit)
          (setq seen (list :prompt prompt
                           :inherit inherit
                           :unread unread-command-events
                           :override overriding-terminal-local-map))
          "日本語")
      (isearch-printing-char ?n 1)
      (should (equal isearch-string "日本語"))
      (should (= (point) (+ (point-min) (length "今日は日本語"))))
      (should (plist-get seen :inherit))
      (should (equal (plist-get seen :unread) '(?n)))
      (should-not (plist-get seen :override))
      (should (string-prefix-p "I-search" (plist-get seen :prompt))))))

(ert-deftest sekken-isearch/空の入力では検索文字列を変えない ()
  (sekken-isearch-test--with-search
      (lambda (&rest _) "")
    (isearch-printing-char ?n 1)
    (should (equal isearch-string ""))))

(ert-deftest sekken-isearch/C-g_は入力だけを取り消し検索を続ける ()
  (sekken-isearch-test--with-search
      (lambda (&rest _) (signal 'quit nil))
    (isearch-printing-char ?n 1)
    (should isearch-mode)
    (should (equal isearch-string ""))))

(ert-deftest sekken-isearch/sekken_以外の入力方式には関わらない ()
  (let (called)
    (with-temp-buffer
      (set-window-buffer nil (current-buffer))
      (cl-letf (((symbol-function 'read-string)
                 (lambda (&rest _) (setq called t) "")))
        (isearch-mode t)
        (unwind-protect
            (let ((this-command 'isearch-printing-char))
              (isearch-printing-char ?n 1)
              (should (equal isearch-string "n")))
          (isearch-done))))
    (should-not called)))

(provide 'sekken-isearch-test)
;;; sekken-isearch-test.el ends here

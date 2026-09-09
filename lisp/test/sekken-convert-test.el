;;; sekken-convert-test.el --- sekken-convert のテスト -*- lexical-binding: t -*-

;; SPDX-License-Identifier: MIT
;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'sekken-convert)

(defmacro sekken-convert-test--with-engine (candidates &rest body)
  "エンジン呼び出しを CANDIDATES を返す偽物に差し替えて BODY を実行する。"
  (declare (indent 1))
  `(cl-letf (((symbol-function 'sekken-server-henkan)
              (lambda (_input _top) ,candidates)))
     (setq sekken-convert--cache nil)
     ,@body))

(ert-deftest sekken-convert/大文字が無ければひらがなに置き換える ()
  (with-temp-buffer
    (insert "kyouha")
    (sekken-convert-test--with-engine nil
      (sekken-convert))
    (should (equal (buffer-string) "きょうは"))
    (should (= (point) (point-max)))))

(ert-deftest sekken-convert/大文字があれば候補を_completion-in-region_に渡す ()
  (with-temp-buffer
    (insert "Neko")
    (let (called)
      (sekken-convert-test--with-engine '("猫" "ねこ")
        (cl-letf (((symbol-function 'completion-in-region)
                   (lambda (start end table &optional _pred)
                     (setq called (list start end (all-completions "" table))))))
          (sekken-convert)))
      (should (equal called '(1 5 ("猫" "ねこ")))))))

(ert-deftest sekken-convert/入力が無ければ元のキーのコマンドを実行する ()
  (with-temp-buffer
    (insert "abc ")
    (cl-letf (((symbol-function 'sekken-convert--fallback-command)
               (lambda () #'newline)))
      (sekken-convert))
    ;; `newline' は electric-indent-mode で直前の空白を消す。
    (should (equal (buffer-string) "abc\n"))))

(ert-deftest sekken-convert/補完テーブルは入力に関係なく候補を全件返す ()
  (sekken-convert-test--with-engine '("猫" "ねこ")
    (should (equal (all-completions "Neko" #'sekken-convert-table) '("猫" "ねこ")))
    (should (equal (try-completion "Neko" #'sekken-convert-table) "Neko"))
    (should (test-completion "猫" #'sekken-convert-table))))

(ert-deftest sekken-convert/capf_は大文字境界を含む語だけに反応する ()
  (with-temp-buffer
    (insert "neko")
    (should (null (sekken-completion-at-point)))
    (insert " Neko")
    (let ((capf (sekken-completion-at-point)))
      (should (= (nth 0 capf) 6))
      (should (= (nth 1 capf) 10))
      (should (eq (nth 2 capf) #'sekken-convert-table)))))

(provide 'sekken-convert-test)
;;; sekken-convert-test.el ends here
